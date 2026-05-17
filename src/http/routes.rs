//! HTTP route handlers for the relay.
//!
//! Handlers authenticate requests, call service traits, and serialize response
//! DTOs. WebSocket transport will live in a separate module.

use crate::http::desktop_auth_headers::desktop_auth_proof;
use crate::http::errors::HttpError;
use crate::http::services::AppServices;
use crate::rooms::{ConnectionId, InviteCode, RoomView};
use crate::transport::{WebSocketJoinRequest, WebSocketJoinRole, handle_websocket_session};
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

const NETPLAY_FEATURE: &str = "netplay";

/// Builds the HTTP router for the relay server.
pub fn build_router(services: AppServices) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/rooms", post(create_room))
        .route("/v1/rooms/{invite_code}/status", get(room_status))
        .route("/v1/ws", get(websocket_room))
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
) -> Result<Json<CreateRoomResponse>, HttpError> {
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    let auth = desktop_auth_proof(&headers, "POST", path_and_query)?;
    let license = services
        .license_authority
        .verify_desktop_access(auth, NETPLAY_FEATURE)
        .await?;
    let room = services
        .rooms
        .create_room(license, ConnectionId::new())
        .await?;

    Ok(Json(CreateRoomResponse { room }))
}

/// Returns status for a room by invite code.
pub async fn room_status(
    State(services): State<AppServices>,
    Path(invite_code): Path<String>,
) -> Result<Json<RoomStatusResponse>, HttpError> {
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
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(uri.path());
    let auth = desktop_auth_proof(&headers, "GET", path_and_query)?;
    let license = services
        .license_authority
        .verify_desktop_access(auth, NETPLAY_FEATURE)
        .await?;
    let invite_code = InviteCode::parse(query.invite_code)?;
    let join_request = WebSocketJoinRequest {
        invite_code,
        role: query.role,
        license,
    };

    Ok(
        websocket
            .on_upgrade(move |socket| handle_websocket_session(socket, services, join_request)),
    )
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketRoomQuery {
    invite_code: String,
    #[serde(default)]
    role: WebSocketJoinRole,
}

#[cfg(test)]
mod tests {
    use super::build_router;
    use crate::auth::{AuthError, DesktopAuthProof, LicenseAuthority, VerifiedLicense};
    use crate::http::services::AppServices;
    use crate::rooms::{InMemoryRoomRegistry, InviteCode, InviteCodeGenerator};
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    struct FakeLicenseAuthority;

    #[async_trait::async_trait]
    impl LicenseAuthority for FakeLicenseAuthority {
        async fn verify_desktop_access(
            &self,
            auth: DesktopAuthProof,
            _feature: &'static str,
        ) -> Result<VerifiedLicense, AuthError> {
            if auth.access_token.expose_secret() == "valid" {
                Ok(VerifiedLicense::with_entitlement(
                    auth.installation_id.as_str(),
                    "subject",
                    "premium",
                    vec!["netplay".to_string()],
                    true,
                    false,
                ))
            } else if auth.access_token.expose_secret() == "expired" {
                Err(AuthError::EntitlementRequired)
            } else {
                Err(AuthError::Unauthorized)
            }
        }
    }

    struct StaticInviteCodeGenerator;

    impl InviteCodeGenerator for StaticInviteCodeGenerator {
        fn generate(&self) -> InviteCode {
            InviteCode::parse("AB23-CD").expect("invite")
        }
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_room_requires_bearer_token() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/rooms")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_room_requires_installation_id() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/rooms")
                    .header("authorization", "Bearer valid")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_room_rejects_expired_entitlement() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/rooms")
                    .header("authorization", "Bearer expired")
                    .header("x-install-id", "install-1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn create_room_returns_invite_code() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/rooms")
                    .header("authorization", "Bearer valid")
                    .header("x-install-id", "install-1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let value = serde_json::from_slice::<Value>(&body).expect("json");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["room"]["inviteCode"], "AB23-CD");
    }

    fn app() -> axum::Router {
        let services = AppServices::new(
            Arc::new(FakeLicenseAuthority),
            Arc::new(InMemoryRoomRegistry::new(Arc::new(
                StaticInviteCodeGenerator,
            ))),
        );

        build_router(services)
    }
}
