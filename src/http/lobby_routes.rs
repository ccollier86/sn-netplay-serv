//! HTTP routes for persistent multiplayer lobbies.
//!
//! Lobby routes stay separate from game-room routes so the new lobby flow can
//! grow without turning the existing relay route module into a catch-all file.

use crate::http::client_auth_headers::client_auth_proof;
use crate::http::client_identity::request_rate_limit_key;
use crate::http::errors::HttpError;
use crate::http::services::AppServices;
use crate::lobbies::{
    CreateLobbyParams, JoinLobbyParams, LobbyClientCapabilities, LobbyJoin, LobbyView,
    MAX_LOBBY_PLAYERS, PublicLobbySummary,
};
use crate::protocol::{NetplayClientKind, validate_client_protocol_version};
use crate::rate_limit::RateLimitAction;
use crate::rooms::PlayerVoiceJoinGrant;
use crate::rooms::{InviteCode, PlayerIndex};
use crate::transport::{
    WebSocketLobbyJoinRequest, handle_public_lobbies_websocket_session,
    handle_websocket_lobby_session,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use tracing::warn;

const NETPLAY_FEATURE: &str = "netplay";

/// Creates a persistent multiplayer lobby for the verified host.
pub async fn create_lobby(
    State(services): State<AppServices>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<LobbySessionResponse>, HttpError> {
    enforce_rate_limit(&services, RateLimitAction::CreateRoom, &headers)?;
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    let auth = client_auth_proof(&headers, "POST", path_and_query, &body)?;
    let license = match services
        .license_authority
        .verify_client_access(auth, NETPLAY_FEATURE)
        .await
    {
        Ok(license) => license,
        Err(error) => {
            services.metrics.record_auth_rejected();
            return Err(error.into());
        }
    };
    let request = parse_create_lobby_request(&body)?;
    validate_client_protocol_version(request.protocol_version)?;
    services.protocol_rollout.validate_exact(
        netplay_client_kind(license.client_kind),
        request.protocol_version,
    )?;
    let join = services
        .lobbies
        .create_lobby(license, request.params)
        .await?;

    Ok(Json(LobbySessionResponse::from(join)))
}

/// Adds the verified player to an existing persistent multiplayer lobby.
pub async fn join_lobby(
    State(services): State<AppServices>,
    Path(invite_code): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<LobbySessionResponse>, HttpError> {
    enforce_rate_limit(&services, RateLimitAction::WebSocketJoin, &headers)?;
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    let auth = client_auth_proof(&headers, "POST", path_and_query, &body)?;
    let license = match services
        .license_authority
        .verify_client_access(auth, NETPLAY_FEATURE)
        .await
    {
        Ok(license) => license,
        Err(error) => {
            services.metrics.record_auth_rejected();
            return Err(error.into());
        }
    };
    let request = parse_join_lobby_request(&body)?;
    validate_client_protocol_version(request.protocol_version)?;
    services.protocol_rollout.validate_exact(
        netplay_client_kind(license.client_kind),
        request.protocol_version,
    )?;
    let invite_code = InviteCode::parse(invite_code)?;
    let join = services
        .lobbies
        .join_lobby(invite_code, license, request.params)
        .await?;

    Ok(Json(LobbySessionResponse::from(join)))
}

/// Returns status for a lobby by invite code.
pub async fn lobby_status(
    State(services): State<AppServices>,
    Path(invite_code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<LobbyStatusResponse>, HttpError> {
    enforce_rate_limit(&services, RateLimitAction::RoomStatus, &headers)?;
    let invite_code = InviteCode::parse(invite_code)?;
    let lobby = services.lobbies.lobby_view(invite_code).await?;

    Ok(Json(LobbyStatusResponse { lobby }))
}

/// Returns public lobby summaries for signed-in desktop clients.
pub async fn public_lobbies(
    State(services): State<AppServices>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<PublicLobbyListResponse>, HttpError> {
    enforce_rate_limit(&services, RateLimitAction::RoomStatus, &headers)?;
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    let auth = client_auth_proof(&headers, "GET", path_and_query, &[])?;

    if let Err(error) = services
        .license_authority
        .verify_client_access(auth, NETPLAY_FEATURE)
        .await
    {
        services.metrics.record_auth_rejected();
        return Err(error.into());
    }

    Ok(Json(PublicLobbyListResponse {
        lobbies: services.lobbies.public_lobbies().await,
    }))
}

/// Upgrades an authenticated ShadowBoy client into the public lobby directory.
pub async fn websocket_public_lobbies(
    websocket: WebSocketUpgrade,
    State(services): State<AppServices>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    enforce_rate_limit(&services, RateLimitAction::WebSocketJoin, &headers)?;
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    let auth = client_auth_proof(&headers, "GET", path_and_query, &[])?;

    if let Err(error) = services
        .license_authority
        .verify_client_access(auth, NETPLAY_FEATURE)
        .await
    {
        services.metrics.record_auth_rejected();
        return Err(error.into());
    }

    Ok(websocket
        .max_message_size(crate::limits::MAX_WEBSOCKET_MESSAGE_BYTES)
        .max_frame_size(crate::limits::MAX_WEBSOCKET_FRAME_BYTES)
        .on_upgrade(move |socket| handle_public_lobbies_websocket_session(socket, services)))
}

/// Upgrades an authenticated ShadowBoy client into a lobby WebSocket.
pub async fn websocket_lobby(
    websocket: WebSocketUpgrade,
    State(services): State<AppServices>,
    Query(query): Query<WebSocketLobbyQuery>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    enforce_rate_limit(&services, RateLimitAction::WebSocketJoin, &headers)?;
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    let auth = client_auth_proof(&headers, "GET", path_and_query, &[])?;
    let license = match services
        .license_authority
        .verify_client_access(auth, NETPLAY_FEATURE)
        .await
    {
        Ok(license) => license,
        Err(error) => {
            services.metrics.record_auth_rejected();
            return Err(error.into());
        }
    };
    let protocol_version = query.protocol_version.ok_or(HttpError::InvalidRequest {
        code: "missingProtocolVersion",
        message: "Netplay protocol version is required.",
    })?;
    validate_client_protocol_version(protocol_version)?;
    services
        .protocol_rollout
        .validate_exact(netplay_client_kind(license.client_kind), protocol_version)?;
    let invite_code = InviteCode::parse(&query.invite_code)?;
    let reconnect = reconnect_query(&query)?;
    let capabilities = lobby_capabilities(&query, license.client_kind);
    let join_request = WebSocketLobbyJoinRequest {
        invite_code,
        display_name: query.display_name,
        capabilities,
        reconnect_player_index: reconnect.as_ref().map(|value| value.player_index),
        reconnect_lobby_epoch: reconnect.as_ref().map(|value| value.lobby_epoch),
        resume_token: reconnect.map(|value| value.resume_token),
        license,
    };

    Ok(websocket
        .max_message_size(crate::limits::MAX_WEBSOCKET_MESSAGE_BYTES)
        .max_frame_size(crate::limits::MAX_WEBSOCKET_FRAME_BYTES)
        .on_upgrade(move |socket| handle_websocket_lobby_session(socket, services, join_request)))
}

fn netplay_client_kind(client_kind: crate::auth::ClientKind) -> NetplayClientKind {
    match client_kind {
        crate::auth::ClientKind::Desktop => NetplayClientKind::Desktop,
        crate::auth::ClientKind::Android => NetplayClientKind::Android,
        crate::auth::ClientKind::Ios => NetplayClientKind::Ios,
    }
}

fn parse_create_lobby_request(body: &[u8]) -> Result<CreateLobbyRequest, HttpError> {
    serde_json::from_slice::<CreateLobbyRequest>(body).map_err(|_| HttpError::InvalidRequest {
        code: "invalidCreateLobbyRequest",
        message: "Create-lobby request JSON is invalid.",
    })
}

fn parse_join_lobby_request(body: &[u8]) -> Result<JoinLobbyRequest, HttpError> {
    serde_json::from_slice::<JoinLobbyRequest>(body).map_err(|_| HttpError::InvalidRequest {
        code: "invalidJoinLobbyRequest",
        message: "Join-lobby request JSON is invalid.",
    })
}

fn enforce_rate_limit(
    services: &AppServices,
    action: RateLimitAction,
    headers: &HeaderMap,
) -> Result<(), HttpError> {
    let key = request_rate_limit_key(headers, services.trust_proxy_headers);

    match services.rate_limiter.check(action, &key) {
        Ok(()) => Ok(()),
        Err(error) => {
            warn!(
                action = action.as_str(),
                "rate limit rejected lobby request"
            );
            services.metrics.record_rate_limited();
            Err(error.into())
        }
    }
}

/// Lobby create/join response body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbySessionResponse {
    /// Current lobby state.
    pub lobby: LobbyView,
    /// Assigned zero-based player slot.
    pub player_index: u8,
    /// One-use resume token for this player's lobby slot.
    pub resume_token: String,
    /// Optional private voice grant for this player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<PlayerVoiceJoinGrant>,
}

impl From<LobbyJoin> for LobbySessionResponse {
    fn from(value: LobbyJoin) -> Self {
        Self {
            lobby: value.lobby,
            player_index: value.player_index.zero_based(),
            resume_token: value.resume_token,
            voice: value.voice,
        }
    }
}

/// Lobby status response body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyStatusResponse {
    /// Current lobby view.
    pub lobby: LobbyView,
}

/// Public lobby list response.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicLobbyListResponse {
    /// Current public lobbies, newest first.
    pub lobbies: Vec<PublicLobbySummary>,
}

/// Create-lobby request body supplied by Desktop.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLobbyRequest {
    /// Netplay protocol version.
    pub protocol_version: u16,
    /// Lobby creation parameters.
    #[serde(flatten)]
    pub params: CreateLobbyParams,
}

/// Join-lobby request body supplied by Desktop.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinLobbyRequest {
    /// Netplay protocol version.
    pub protocol_version: u16,
    /// Lobby join parameters.
    #[serde(flatten)]
    pub params: JoinLobbyParams,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketLobbyQuery {
    invite_code: String,
    protocol_version: Option<u16>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    supports_temporary_session_rom_relay: Option<bool>,
    #[serde(default)]
    supports_lobby_voice: Option<bool>,
    #[serde(default)]
    supports_multi_game_lobby: Option<bool>,
    #[serde(default)]
    supports_lobby_returned_event: Option<bool>,
    #[serde(default)]
    supports_lobby_gameplay_started: Option<bool>,
    #[serde(default)]
    supports_lobby_player_removed_event: Option<bool>,
    #[serde(default)]
    player_index: Option<u8>,
    #[serde(default)]
    lobby_epoch: Option<u64>,
    #[serde(default)]
    resume_token: Option<String>,
}

struct LobbyReconnectQuery {
    player_index: PlayerIndex,
    lobby_epoch: u64,
    resume_token: String,
}

fn reconnect_query(query: &WebSocketLobbyQuery) -> Result<Option<LobbyReconnectQuery>, HttpError> {
    match (
        query.player_index,
        query.lobby_epoch,
        query.resume_token.as_ref(),
    ) {
        (None, None, None) => Ok(None),
        (Some(player_index), Some(lobby_epoch), Some(resume_token)) => {
            let player_index = PlayerIndex::new(player_index, MAX_LOBBY_PLAYERS).ok_or(
                HttpError::InvalidRequest {
                    code: "invalidPlayerIndex",
                    message: "Lobby reconnect playerIndex is invalid.",
                },
            )?;

            Ok(Some(LobbyReconnectQuery {
                player_index,
                lobby_epoch,
                resume_token: resume_token.clone(),
            }))
        }
        _ => Err(HttpError::InvalidRequest {
            code: "invalidLobbyReconnectRequest",
            message: "Lobby reconnect requires playerIndex, lobbyEpoch, and resumeToken.",
        }),
    }
}

fn lobby_capabilities(
    query: &WebSocketLobbyQuery,
    client_kind: crate::auth::ClientKind,
) -> LobbyClientCapabilities {
    let defaults_to_rich_desktop = client_kind == crate::auth::ClientKind::Desktop;

    LobbyClientCapabilities {
        supports_lobby: true,
        supports_temporary_session_rom_relay: query
            .supports_temporary_session_rom_relay
            .unwrap_or(defaults_to_rich_desktop),
        supports_lobby_voice: query
            .supports_lobby_voice
            .unwrap_or(defaults_to_rich_desktop),
        supports_multi_game_lobby: query
            .supports_multi_game_lobby
            .unwrap_or(defaults_to_rich_desktop),
        supports_lobby_returned_event: query.supports_lobby_returned_event.unwrap_or(false),
        supports_lobby_gameplay_started: query.supports_lobby_gameplay_started.unwrap_or(false),
        supports_lobby_player_removed_event: query
            .supports_lobby_player_removed_event
            .unwrap_or(false),
    }
}
