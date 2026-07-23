//! HTTP routes for persistent multiplayer lobbies.
//!
//! Lobby routes stay separate from game-room routes so the new lobby flow can
//! grow without turning the existing relay route module into a catch-all file.

use crate::http::client_auth_headers::client_auth_proof;
use crate::http::client_identity::request_rate_limit_key;
use crate::http::errors::HttpError;
use crate::http::services::AppServices;
use crate::lobbies::{
    CreateLobbyParams, JoinLobbyParams, LobbyClientCapabilities, LobbyJoin,
    LobbyLinkCableClientCapabilities, LobbyLinkProtocolFamily, LobbyView, MAX_LOBBY_PLAYERS,
    PublicLobbySummary,
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

    Ok(Json(LobbyStatusResponse {
        lobby: unauthenticated_lobby_status_view(lobby),
    }))
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
    let capabilities = lobby_capabilities(&query, license.client_kind)?;
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
    link_cable_contract_version: Option<u16>,
    #[serde(default)]
    link_cable_runtime_profile: Option<String>,
    #[serde(default)]
    link_cable_core_build_id: Option<String>,
    #[serde(default)]
    link_cable_protocol_families: Option<String>,
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
) -> Result<LobbyClientCapabilities, HttpError> {
    let defaults_to_rich_desktop = client_kind == crate::auth::ClientKind::Desktop;

    Ok(LobbyClientCapabilities {
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
        link_cable: link_cable_capabilities(query)?,
    })
}

fn link_cable_capabilities(
    query: &WebSocketLobbyQuery,
) -> Result<Option<LobbyLinkCableClientCapabilities>, HttpError> {
    let supplied_fields = [
        query.link_cable_contract_version.is_some(),
        query.link_cable_runtime_profile.is_some(),
        query.link_cable_core_build_id.is_some(),
        query.link_cable_protocol_families.is_some(),
    ];
    if supplied_fields.iter().all(|supplied| !supplied) {
        return Ok(None);
    }
    if supplied_fields.iter().any(|supplied| !supplied) {
        return Err(invalid_link_cable_capability());
    }

    let runtime_profile = query
        .link_cable_runtime_profile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid_link_cable_capability)?;
    let core_build_id = query
        .link_cable_core_build_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid_link_cable_capability)?;
    let raw_families = query
        .link_cable_protocol_families
        .as_deref()
        .ok_or_else(invalid_link_cable_capability)?;
    let mut protocol_families = Vec::new();
    for raw_family in raw_families.split(',') {
        let family = match raw_family.trim() {
            "gbSerialV1" => LobbyLinkProtocolFamily::GbSerialV1,
            "gbaMultiV1" => LobbyLinkProtocolFamily::GbaMultiV1,
            _ => return Err(invalid_link_cable_capability()),
        };
        if !protocol_families.contains(&family) {
            protocol_families.push(family);
        }
    }
    if protocol_families.is_empty() {
        return Err(invalid_link_cable_capability());
    }

    Ok(Some(LobbyLinkCableClientCapabilities {
        contract_version: query
            .link_cable_contract_version
            .ok_or_else(invalid_link_cable_capability)?,
        runtime_profile: runtime_profile.to_string(),
        core_build_id: core_build_id.to_string(),
        protocol_families,
    }))
}

fn invalid_link_cable_capability() -> HttpError {
    HttpError::InvalidRequest {
        code: "invalidLinkCableCapability",
        message: "Link-cable lobby capability fields are incomplete or invalid.",
    }
}

fn unauthenticated_lobby_status_view(mut lobby: LobbyView) -> LobbyView {
    lobby.multiplayer_extension = None;
    lobby
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_link_capability_is_absent_for_legacy_queries() {
        let capabilities =
            lobby_capabilities(&query(), crate::auth::ClientKind::Android).expect("capabilities");

        assert!(capabilities.link_cable.is_none());
    }

    #[test]
    fn websocket_link_capability_parses_the_complete_kotlin_query_contract() {
        let mut query = query();
        query.link_cable_contract_version = Some(1);
        query.link_cable_runtime_profile = Some("mgba-link-runtime-v1".to_string());
        query.link_cable_core_build_id = Some("android-mgba-link-v1".to_string());
        query.link_cable_protocol_families = Some("gbSerialV1,gbaMultiV1".to_string());

        let link = lobby_capabilities(&query, crate::auth::ClientKind::Android)
            .expect("capabilities")
            .link_cable
            .expect("link capability");

        assert_eq!(link.contract_version, 1);
        assert_eq!(link.runtime_profile, "mgba-link-runtime-v1");
        assert_eq!(link.core_build_id, "android-mgba-link-v1");
        assert_eq!(
            link.protocol_families,
            vec![
                LobbyLinkProtocolFamily::GbSerialV1,
                LobbyLinkProtocolFamily::GbaMultiV1,
            ],
        );
    }

    #[test]
    fn websocket_link_capability_fails_closed_when_partial_or_unknown() {
        let mut partial = query();
        partial.link_cable_contract_version = Some(1);
        assert!(matches!(
            lobby_capabilities(&partial, crate::auth::ClientKind::Android),
            Err(HttpError::InvalidRequest {
                code: "invalidLinkCableCapability",
                ..
            })
        ));

        let mut unknown = query();
        unknown.link_cable_contract_version = Some(1);
        unknown.link_cable_runtime_profile = Some("mgba-link-runtime-v1".to_string());
        unknown.link_cable_core_build_id = Some("android-mgba-link-v1".to_string());
        unknown.link_cable_protocol_families = Some("futureFamily".to_string());
        assert!(matches!(
            lobby_capabilities(&unknown, crate::auth::ClientKind::Android),
            Err(HttpError::InvalidRequest {
                code: "invalidLinkCableCapability",
                ..
            })
        ));
    }

    #[test]
    fn unauthenticated_lobby_status_strips_specialized_room_invite_extension() {
        let view = LobbyView {
            lobby_id: crate::rooms::RoomId::new(),
            event_seq: 4,
            lobby_epoch: 3,
            invite_code: "AB23-CD".to_string(),
            created_at_ms: 1,
            updated_at_ms: 2,
            last_meaningful_activity_at_ms: 2,
            status: crate::lobbies::LobbyStatus::GameSelected,
            visibility: crate::lobbies::LobbyVisibility::Private,
            capabilities: crate::lobbies::LobbyServerCapabilities::current(2, false, false),
            players: Vec::new(),
            selected_game: None,
            game_readiness: Vec::new(),
            pending_launch: None,
            voice: None,
            multiplayer_extension: Some(crate::lobbies::LobbyMultiplayerExtension {
                session_kind: crate::lobbies::LobbyMultiplayerSessionKind::LinkCable,
                generation: 1,
                link_cable: Some(crate::lobbies::LobbyLinkCableView {
                    protocol_family: LobbyLinkProtocolFamily::GbaMultiV1,
                    max_players: 2,
                    room_invite_code: Some("EF45-GH".to_string()),
                    cable_epoch: None,
                    players: Vec::new(),
                }),
            }),
        };

        let projected = unauthenticated_lobby_status_view(view);

        assert!(projected.multiplayer_extension.is_none());
        let payload = serde_json::to_value(projected).expect("status serializes");
        assert!(payload.get("multiplayerExtension").is_none());
    }

    fn query() -> WebSocketLobbyQuery {
        WebSocketLobbyQuery {
            invite_code: "AB23-CD".to_string(),
            protocol_version: Some(crate::protocol::NETPLAY_PROTOCOL_VERSION),
            display_name: None,
            supports_temporary_session_rom_relay: None,
            supports_lobby_voice: None,
            supports_multi_game_lobby: None,
            supports_lobby_returned_event: None,
            supports_lobby_gameplay_started: None,
            supports_lobby_player_removed_event: None,
            link_cable_contract_version: None,
            link_cable_runtime_profile: None,
            link_cable_core_build_id: None,
            link_cable_protocol_families: None,
            player_index: None,
            lobby_epoch: None,
            resume_token: None,
        }
    }
}
