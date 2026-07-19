//! HTTP route handlers for the relay.
//!
//! Handlers authenticate requests, call service traits, and serialize response
//! DTOs. WebSocket transport will live in a separate module.

use crate::http::client_auth_headers::client_auth_proof;
use crate::http::client_identity::{capability_request_rate_limit_key, request_rate_limit_key};
use crate::http::errors::HttpError;
use crate::http::lobby_routes::{
    LobbyStatusResponse, create_lobby, join_lobby, lobby_status, public_lobbies, websocket_lobby,
    websocket_public_lobbies,
};
use crate::http::services::AppServices;
use crate::limits::{
    MAX_CREATE_ROOM_BODY_BYTES, MAX_WEBSOCKET_FRAME_BYTES, MAX_WEBSOCKET_MESSAGE_BYTES,
};
use crate::lobbies::{LobbyDebugEvent, LobbyRegistrySnapshot};
use crate::observability::MetricsSnapshot;
use crate::protocol::{
    NetplayClientKind, NetplaySessionDescriptor, validate_client_protocol_version,
};
use crate::rate_limit::RateLimitAction;
use crate::rooms::{ConnectionId, InviteCode, PlayerIndex, RoomView};
use crate::rooms::{RoomDebugEvent, RoomRegistrySnapshot};
use crate::transport::{
    WebSocketInputJoinRequest, WebSocketJoinRequest, WebSocketJoinRole, WebSocketRoomJoinIntent,
    handle_websocket_input_session, handle_websocket_session,
};
use axum::body::Bytes;
use axum::extract::DefaultBodyLimit;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use tracing::warn;

const NETPLAY_FEATURE: &str = "netplay";

/// Builds the HTTP router for the relay server.
pub fn build_router(services: AppServices) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/rooms", post(create_room))
        .route("/v1/rooms/{invite_code}/status", get(room_status))
        .route("/v1/lobbies", post(create_lobby))
        .route("/v1/lobbies/public", get(public_lobbies))
        .route("/v1/lobbies/public/ws", get(websocket_public_lobbies))
        .route("/v1/lobbies/{invite_code}/join", post(join_lobby))
        .route("/v1/lobbies/{invite_code}/status", get(lobby_status))
        .route("/v1/lobbies/ws", get(websocket_lobby))
        .route("/v1/ws", get(websocket_room))
        .route("/v1/ws/input", get(websocket_input_room))
        .route("/internal/metrics", get(internal_metrics))
        .route("/internal/rooms", get(internal_rooms))
        .route("/internal/rooms/{invite_code}", get(internal_room))
        .route(
            "/internal/rooms/{invite_code}/events",
            get(internal_room_events),
        )
        .route("/internal/recent-events", get(internal_recent_events))
        .route("/internal/lobbies", get(internal_lobbies))
        .route("/internal/lobbies/{invite_code}", get(internal_lobby))
        .route(
            "/internal/lobbies/{invite_code}/events",
            get(internal_lobby_events),
        )
        .route(
            "/internal/recent-lobby-events",
            get(internal_recent_lobby_events),
        )
        .layer(DefaultBodyLimit::max(MAX_CREATE_ROOM_BODY_BYTES))
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                tracing::info_span!(
                    "http_request",
                    method = %request.method(),
                    path = %trace_request_path(request)
                )
            }),
        )
        .with_state(services)
}

fn trace_request_path<B>(request: &axum::http::Request<B>) -> &str {
    request.uri().path()
}

/// Returns a simple process health response.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

/// Creates a netplay room for the verified host.
pub async fn create_room(
    State(services): State<AppServices>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<CreateRoomResponse>, HttpError> {
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
    let request = parse_create_room_request(&body)?;
    validate_client_protocol_version(request.desktop_protocol_version)?;
    let mut session = request.session;
    session.host_client_kind = Some(netplay_client_kind(license.client_kind));
    session.rom_relay = None;
    session.validate()?;
    session.rom_relay = services
        .file_relay_policy
        .direct_rom_relay_capability(services.file_relay.as_ref(), &session);
    let room = services
        .rooms
        .create_room(license, ConnectionId::new(), session)
        .await?;

    services.metrics.record_room_created();
    Ok(Json(CreateRoomResponse { room }))
}

/// Returns status for a room by invite code.
pub async fn room_status(
    State(services): State<AppServices>,
    Path(invite_code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RoomStatusResponse>, HttpError> {
    enforce_rate_limit(&services, RateLimitAction::RoomStatus, &headers)?;
    let invite_code = InviteCode::parse(invite_code)?;
    let room = services.rooms.room_view(invite_code).await?;

    Ok(Json(RoomStatusResponse { room }))
}

/// Upgrades a protected initial join or capability-authorized reconnect.
pub async fn websocket_room(
    websocket: WebSocketUpgrade,
    State(services): State<AppServices>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Query(query): Query<WebSocketRoomQuery>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    enforce_capability_rate_limit(&services, &headers, connect_info)?;
    validate_client_protocol_version(query.protocol_version.ok_or(HttpError::InvalidRequest {
        code: "missingProtocolVersion",
        message: "Desktop netplay protocol version is required.",
    })?)?;
    let invite_code = InviteCode::parse(&query.invite_code)?;
    let reconnect = reconnect_query(&query)?;
    let intent = match reconnect {
        Some(reconnect) => {
            if query.runner_handoff {
                return Err(HttpError::InvalidRequest {
                    code: "invalidRunnerHandoffRequest",
                    message: "runnerHandoff is valid only for an initial room join.",
                });
            }

            WebSocketRoomJoinIntent::Resume {
                player_index: reconnect.player_index,
                room_epoch: reconnect.room_epoch,
                resume_token: reconnect.resume_token,
            }
        }
        None => {
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

            WebSocketRoomJoinIntent::Initial {
                role: query.role,
                license,
                runner_handoff: query.runner_handoff,
            }
        }
    };
    let join_request = WebSocketJoinRequest {
        invite_code,
        intent,
        supports_state_file_relay: query.supports_state_file_relay.unwrap_or(false),
        supports_rom_file_relay: query.supports_rom_file_relay.unwrap_or(false),
        supports_scheduled_start: query.supports_scheduled_start.unwrap_or(false),
        supports_clock_sync: query.supports_clock_sync.unwrap_or(false),
        supports_fast_input_relay: query.supports_fast_input_relay.unwrap_or(false),
    };

    Ok(websocket
        .max_message_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .max_frame_size(MAX_WEBSOCKET_FRAME_BYTES)
        .on_upgrade(move |socket| handle_websocket_session(socket, services, join_request)))
}

/// Upgrades a one-time input capability into a binary input socket.
pub async fn websocket_input_room(
    websocket: WebSocketUpgrade,
    State(services): State<AppServices>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Query(query): Query<WebSocketInputRoomQuery>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    enforce_capability_rate_limit(&services, &headers, connect_info)?;
    validate_client_protocol_version(query.protocol_version.ok_or(HttpError::InvalidRequest {
        code: "missingProtocolVersion",
        message: "Netplay protocol version is required.",
    })?)?;
    let invite_code = InviteCode::parse(&query.invite_code)?;
    let player_index = PlayerIndex::new(query.player_index, crate::limits::MVP_ROOM_CAPACITY)
        .ok_or(HttpError::InvalidRequest {
            code: "invalidPlayerIndex",
            message: "Input socket playerIndex is invalid.",
        })?;
    let join_request = WebSocketInputJoinRequest {
        input_socket_token: query.input_socket_token,
        invite_code,
        player_index,
        room_epoch: query.room_epoch,
        session_epoch: query.session_epoch,
    };

    Ok(websocket
        .max_message_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .max_frame_size(MAX_WEBSOCKET_FRAME_BYTES)
        .on_upgrade(move |socket| handle_websocket_input_session(socket, services, join_request)))
}

fn parse_create_room_request(body: &[u8]) -> Result<CreateRoomRequest, HttpError> {
    serde_json::from_slice::<CreateRoomRequest>(body).map_err(|_| HttpError::InvalidRequest {
        code: "invalidCreateRoomRequest",
        message: "Create-room request JSON is invalid.",
    })
}

fn netplay_client_kind(client_kind: crate::auth::ClientKind) -> NetplayClientKind {
    match client_kind {
        crate::auth::ClientKind::Desktop => NetplayClientKind::Desktop,
        crate::auth::ClientKind::Android => NetplayClientKind::Android,
        crate::auth::ClientKind::Ios => NetplayClientKind::Ios,
    }
}

/// Returns internal process metrics for authenticated operators.
pub async fn internal_metrics(
    State(services): State<AppServices>,
    headers: HeaderMap,
) -> Result<Json<MetricsSnapshot>, HttpError> {
    services.admin_authorizer.verify(&headers)?;

    Ok(Json(services.metrics.snapshot()))
}

/// Returns current active room views for authenticated operators.
pub async fn internal_rooms(
    State(services): State<AppServices>,
    headers: HeaderMap,
) -> Result<Json<RoomRegistrySnapshot>, HttpError> {
    services.admin_authorizer.verify(&headers)?;

    Ok(Json(services.rooms.snapshot().await))
}

/// Returns one active room view for authenticated operators.
pub async fn internal_room(
    State(services): State<AppServices>,
    Path(invite_code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RoomStatusResponse>, HttpError> {
    services.admin_authorizer.verify(&headers)?;
    let invite_code = InviteCode::parse(invite_code)?;
    let room = services.rooms.room_view(invite_code).await?;

    Ok(Json(RoomStatusResponse { room }))
}

/// Returns sanitized event history for one active room.
pub async fn internal_room_events(
    State(services): State<AppServices>,
    Path(invite_code): Path<String>,
    Query(query): Query<EventLogQuery>,
    headers: HeaderMap,
) -> Result<Json<EventLogResponse>, HttpError> {
    services.admin_authorizer.verify(&headers)?;
    let invite_code = InviteCode::parse(invite_code)?;
    let events = services
        .rooms
        .room_events(invite_code, query.limit())
        .await?;

    Ok(Json(EventLogResponse { events }))
}

/// Returns sanitized event history across active rooms.
pub async fn internal_recent_events(
    State(services): State<AppServices>,
    Query(query): Query<EventLogQuery>,
    headers: HeaderMap,
) -> Result<Json<EventLogResponse>, HttpError> {
    services.admin_authorizer.verify(&headers)?;
    let events = services.rooms.recent_events(query.limit()).await;

    Ok(Json(EventLogResponse { events }))
}

/// Returns current active lobby views for authenticated operators.
pub async fn internal_lobbies(
    State(services): State<AppServices>,
    headers: HeaderMap,
) -> Result<Json<LobbyRegistrySnapshot>, HttpError> {
    services.admin_authorizer.verify(&headers)?;

    Ok(Json(services.lobbies.snapshot().await))
}

/// Returns one active lobby view for authenticated operators.
pub async fn internal_lobby(
    State(services): State<AppServices>,
    Path(invite_code): Path<String>,
    headers: HeaderMap,
) -> Result<Json<LobbyStatusResponse>, HttpError> {
    services.admin_authorizer.verify(&headers)?;
    let invite_code = InviteCode::parse(invite_code)?;
    let lobby = services.lobbies.lobby_view(invite_code).await?;

    Ok(Json(LobbyStatusResponse { lobby }))
}

/// Returns sanitized event history for one active lobby.
pub async fn internal_lobby_events(
    State(services): State<AppServices>,
    Path(invite_code): Path<String>,
    Query(query): Query<EventLogQuery>,
    headers: HeaderMap,
) -> Result<Json<LobbyEventLogResponse>, HttpError> {
    services.admin_authorizer.verify(&headers)?;
    let invite_code = InviteCode::parse(invite_code)?;
    let events = services
        .lobbies
        .lobby_events(invite_code, query.limit())
        .await?;

    Ok(Json(LobbyEventLogResponse { events }))
}

/// Returns sanitized event history across active lobbies.
pub async fn internal_recent_lobby_events(
    State(services): State<AppServices>,
    Query(query): Query<EventLogQuery>,
    headers: HeaderMap,
) -> Result<Json<LobbyEventLogResponse>, HttpError> {
    services.admin_authorizer.verify(&headers)?;
    let events = services.lobbies.recent_events(query.limit()).await;

    Ok(Json(LobbyEventLogResponse { events }))
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
            warn!(action = action.as_str(), "rate limit rejected request");
            services.metrics.record_rate_limited();
            Err(error.into())
        }
    }
}

fn enforce_capability_rate_limit(
    services: &AppServices,
    headers: &HeaderMap,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
) -> Result<(), HttpError> {
    let peer_ip = connect_info.map(|Extension(ConnectInfo(address))| address.ip());
    let key = capability_request_rate_limit_key(headers, services.trust_proxy_headers, peer_ip);

    match services
        .rate_limiter
        .check(RateLimitAction::WebSocketJoin, &key)
    {
        Ok(()) => Ok(()),
        Err(error) => {
            warn!(
                action = RateLimitAction::WebSocketJoin.as_str(),
                "rate limit rejected request"
            );
            services.metrics.record_rate_limited();
            Err(error.into())
        }
    }
}

/// Health check response body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Static process status.
    pub status: &'static str,
}

/// Room creation response body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoomResponse {
    /// Newly created room view.
    pub room: RoomView,
}

/// Room status response body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomStatusResponse {
    /// Current room view.
    pub room: RoomView,
}

fn reconnect_query(query: &WebSocketRoomQuery) -> Result<Option<ReconnectQuery>, HttpError> {
    match (
        query.player_index,
        query.room_epoch,
        query.resume_token.as_ref(),
    ) {
        (None, None, None) => Ok(None),
        (Some(player_index), Some(room_epoch), Some(resume_token)) => {
            let player_index = PlayerIndex::new(player_index, crate::limits::MVP_ROOM_CAPACITY)
                .ok_or(HttpError::InvalidRequest {
                    code: "invalidPlayerIndex",
                    message: "Reconnect playerIndex is invalid.",
                })?;

            Ok(Some(ReconnectQuery {
                player_index,
                room_epoch,
                resume_token: resume_token.clone(),
            }))
        }
        _ => Err(HttpError::InvalidRequest {
            code: "invalidReconnectRequest",
            message: "Reconnect requires playerIndex, roomEpoch, and resumeToken.",
        }),
    }
}

/// Event log response body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventLogResponse {
    /// Sanitized room events.
    pub events: Vec<RoomDebugEvent>,
}

/// Lobby event log response body.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyEventLogResponse {
    /// Sanitized lobby events.
    pub events: Vec<LobbyDebugEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventLogQuery {
    #[serde(default)]
    limit: Option<usize>,
}

impl EventLogQuery {
    fn limit(&self) -> usize {
        self.limit.unwrap_or(100).clamp(1, 500)
    }
}

/// Create-room request body supplied by Desktop.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoomRequest {
    /// Desktop netplay protocol version.
    pub desktop_protocol_version: u16,
    /// Game/core details for invite preview and ROM matching.
    pub session: NetplaySessionDescriptor,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketRoomQuery {
    invite_code: String,
    #[serde(default)]
    role: WebSocketJoinRole,
    #[serde(default)]
    protocol_version: Option<u16>,
    #[serde(default)]
    player_index: Option<u8>,
    #[serde(default)]
    room_epoch: Option<u64>,
    #[serde(default)]
    resume_token: Option<String>,
    #[serde(default)]
    runner_handoff: bool,
    #[serde(default)]
    supports_state_file_relay: Option<bool>,
    #[serde(default)]
    supports_rom_file_relay: Option<bool>,
    #[serde(default)]
    supports_scheduled_start: Option<bool>,
    #[serde(default)]
    supports_clock_sync: Option<bool>,
    #[serde(default)]
    supports_fast_input_relay: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketInputRoomQuery {
    invite_code: String,
    protocol_version: Option<u16>,
    player_index: u8,
    room_epoch: u64,
    session_epoch: u64,
    input_socket_token: String,
}

struct ReconnectQuery {
    player_index: PlayerIndex,
    room_epoch: u64,
    resume_token: String,
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod routes_tests;
