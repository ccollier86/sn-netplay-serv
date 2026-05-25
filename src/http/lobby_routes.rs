//! HTTP routes for persistent multiplayer lobbies.
//!
//! Lobby routes stay separate from game-room routes so the new lobby flow can
//! grow without turning the existing relay route module into a catch-all file.

use crate::http::client_auth_headers::client_auth_proof;
use crate::http::client_identity::request_rate_limit_key;
use crate::http::errors::HttpError;
use crate::http::services::AppServices;
use crate::lobbies::{CreateLobbyParams, JoinLobbyParams, LobbyJoin, LobbyView};
use crate::protocol::validate_client_protocol_version;
use crate::rate_limit::RateLimitAction;
use crate::rooms::InviteCode;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
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
}

impl From<LobbyJoin> for LobbySessionResponse {
    fn from(value: LobbyJoin) -> Self {
        Self {
            lobby: value.lobby,
            player_index: value.player_index.zero_based(),
            resume_token: value.resume_token,
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
