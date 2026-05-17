//! HTTP route handlers for the relay.
//!
//! Handlers authenticate requests, call service traits, and serialize response
//! DTOs. WebSocket transport will live in a separate module.

use crate::http::client_identity::request_rate_limit_key;
use crate::http::desktop_auth_headers::desktop_auth_proof;
use crate::http::errors::HttpError;
use crate::http::services::AppServices;
use crate::limits::{
    MAX_CREATE_ROOM_BODY_BYTES, MAX_WEBSOCKET_FRAME_BYTES, MAX_WEBSOCKET_MESSAGE_BYTES,
};
use crate::observability::MetricsSnapshot;
use crate::protocol::{NetplaySessionDescriptor, validate_client_protocol_version};
use crate::rate_limit::RateLimitAction;
use crate::rooms::RoomRegistrySnapshot;
use crate::rooms::{ConnectionId, InviteCode, RoomView};
use crate::transport::{WebSocketJoinRequest, WebSocketJoinRole, handle_websocket_session};
use axum::body::Bytes;
use axum::extract::DefaultBodyLimit;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;
use tracing::warn;

const NETPLAY_FEATURE: &str = "netplay";

/// Builds the HTTP router for the relay server.
pub fn build_router(services: AppServices) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/rooms", post(create_room))
        .route("/v1/rooms/{invite_code}/status", get(room_status))
        .route("/v1/ws", get(websocket_room))
        .route("/internal/metrics", get(internal_metrics))
        .route("/internal/rooms", get(internal_rooms))
        .layer(DefaultBodyLimit::max(MAX_CREATE_ROOM_BODY_BYTES))
        .layer(TraceLayer::new_for_http())
        .with_state(services)
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
    let auth = desktop_auth_proof(&headers, "POST", path_and_query, &body)?;
    let license = match services
        .license_authority
        .verify_desktop_access(auth, NETPLAY_FEATURE)
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
    request.session.validate()?;
    let room = services
        .rooms
        .create_room(license, ConnectionId::new(), request.session)
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

/// Upgrades an authenticated Desktop client into a room WebSocket.
pub async fn websocket_room(
    websocket: WebSocketUpgrade,
    State(services): State<AppServices>,
    Query(query): Query<WebSocketRoomQuery>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    enforce_rate_limit(&services, RateLimitAction::WebSocketJoin, &headers)?;
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    let auth = desktop_auth_proof(&headers, "GET", path_and_query, &[])?;
    let license = match services
        .license_authority
        .verify_desktop_access(auth, NETPLAY_FEATURE)
        .await
    {
        Ok(license) => license,
        Err(error) => {
            services.metrics.record_auth_rejected();
            return Err(error.into());
        }
    };
    validate_client_protocol_version(query.protocol_version.ok_or(HttpError::InvalidRequest {
        code: "missingProtocolVersion",
        message: "Desktop netplay protocol version is required.",
    })?)?;
    let invite_code = InviteCode::parse(query.invite_code)?;
    let join_request = WebSocketJoinRequest {
        invite_code,
        role: query.role,
        license,
    };

    Ok(websocket
        .max_message_size(MAX_WEBSOCKET_MESSAGE_BYTES)
        .max_frame_size(MAX_WEBSOCKET_FRAME_BYTES)
        .on_upgrade(move |socket| handle_websocket_session(socket, services, join_request)))
}

fn parse_create_room_request(body: &[u8]) -> Result<CreateRoomRequest, HttpError> {
    serde_json::from_slice::<CreateRoomRequest>(body).map_err(|_| HttpError::InvalidRequest {
        code: "invalidCreateRoomRequest",
        message: "Create-room request JSON is invalid.",
    })
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
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod routes_tests;
