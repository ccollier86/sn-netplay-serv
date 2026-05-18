//! Tests for HTTP route contracts.
//!
//! These cover auth ordering, create-room descriptors, rate limits, and
//! internal admin observability without growing the production route module.

use super::build_router;
use crate::auth::{
    AuthError, ClientKind, LicenseAuthority, ProtectedClientAuthProof, VerifiedLicense,
};
use crate::http::AdminAuthorizer;
use crate::http::services::AppServices;
use crate::limits::MAX_CREATE_ROOM_BODY_BYTES;
use crate::observability::InMemoryMetrics;
use crate::rate_limit::{InMemoryRateLimiter, RateLimitPolicy};
use crate::rooms::{InMemoryRoomRegistry, InviteCode, InviteCodeGenerator};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

struct FakeLicenseAuthority;

#[async_trait::async_trait]
impl LicenseAuthority for FakeLicenseAuthority {
    async fn verify_client_access(
        &self,
        auth: ProtectedClientAuthProof,
        _feature: &'static str,
    ) -> Result<VerifiedLicense, AuthError> {
        if auth.access_token.expose_secret() == "valid" {
            Ok(VerifiedLicense::with_entitlement(
                auth.client_kind,
                auth.installation_id.as_str(),
                "subject",
                match auth.client_kind {
                    ClientKind::Desktop => "premium",
                    ClientKind::Android => "authenticated",
                },
                vec!["netplay".to_string()],
                auth.client_kind == ClientKind::Desktop,
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
async fn create_room_requires_bearer_token_before_body_validation() {
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
                .body(Body::from(create_room_body()))
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
                .body(Body::from(create_room_body()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn create_room_returns_invite_descriptor_and_protocol() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rooms")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "install-1")
                .body(Body::from(create_room_body()))
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
    assert_eq!(value["room"]["protocol"]["protocolVersion"], 1);
    assert_eq!(
        value["room"]["session"]["game"]["romSha256"],
        "a".repeat(64)
    );
    assert_eq!(value["room"]["session"]["core"]["coreId"], "dolphin");
}

#[tokio::test]
async fn create_room_accepts_android_client_auth() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rooms")
                .header("authorization", "Bearer valid")
                .header("x-client-kind", "android")
                .header("x-installation-id", "android-install-1")
                .body(Body::from(create_room_body()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_room_accepts_link_cable_descriptor() {
    let mut body = create_room_value();
    body["session"]["mode"] = json!("linkCable");
    body["session"]["link"] = json!({
        "systemFamily": "gba",
        "linkProtocol": "gba-link-cable-v1",
        "runtimeProfile": "mgba-link-runtime-v1",
        "maxPlayers": 2,
        "transport": "relay"
    });
    body["session"]["game"]["systemId"] = json!("gba");
    body["session"]["core"]["coreId"] = json!("mgba");

    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rooms")
                .header("authorization", "Bearer valid")
                .header("x-client-kind", "android")
                .header("x-installation-id", "android-install-1")
                .body(Body::from(body.to_string()))
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
    assert_eq!(value["room"]["session"]["mode"], "linkCable");
    assert_eq!(
        value["room"]["session"]["link"]["runtimeProfile"],
        "mgba-link-runtime-v1"
    );
}

#[tokio::test]
async fn create_room_rejects_unknown_client_kind() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rooms")
                .header("authorization", "Bearer valid")
                .header("x-client-kind", "console")
                .header("x-install-id", "install-1")
                .body(Body::from(create_room_body()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_room_rejects_invalid_descriptor() {
    let mut body = create_room_value();
    body["session"]["game"]["romSha256"] = json!("bad");
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rooms")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "install-1")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_room_rejects_oversized_body_before_parsing() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rooms")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "install-1")
                .body(Body::from(vec![b'{'; MAX_CREATE_ROOM_BODY_BYTES + 1]))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn create_room_rate_limit_returns_429() {
    let response = limited_app(0)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rooms")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "install-1")
                .body(Body::from(create_room_body()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn websocket_join_requires_protocol_version() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/v1/ws?inviteCode=AB23-CD&role=guest")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "install-1")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn internal_metrics_requires_admin_token() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/internal/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn internal_rooms_returns_snapshot_for_admin() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/internal/rooms")
                .header("authorization", "Bearer admin-token")
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
    assert_eq!(value["activeRoomCount"], 0);
}

fn app() -> axum::Router {
    limited_app(12)
}

fn limited_app(create_room_per_minute: u32) -> axum::Router {
    let services = AppServices::new(
        Arc::new(FakeLicenseAuthority),
        Arc::new(InMemoryRoomRegistry::new(Arc::new(
            StaticInviteCodeGenerator,
        ))),
        Arc::new(InMemoryRateLimiter::new(RateLimitPolicy {
            create_room_per_minute,
            websocket_join_per_minute: 30,
            room_status_per_minute: 120,
        })),
        Arc::new(InMemoryMetrics::new()),
        AdminAuthorizer::new(Some("admin-token".to_string())),
        false,
    );

    build_router(services)
}

fn create_room_body() -> String {
    create_room_value().to_string()
}

fn create_room_value() -> Value {
    json!({
        "desktopProtocolVersion": 1,
        "session": {
            "hostAppVersion": "0.3.0",
            "game": {
                "systemId": "gamecube",
                "title": "Star Fox Adventures",
                "romSha256": "a".repeat(64),
                "contentKey": "gamecube-star-fox-adventures-usa",
                "region": "USA",
                "revision": "Rev 1",
                "discId": "GFSE01"
            },
            "core": {
                "coreId": "dolphin",
                "coreName": "Dolphin",
                "coreVersion": "5.0-netplay",
                "coreOptionsSha256": "b".repeat(64)
            }
        }
    })
}
