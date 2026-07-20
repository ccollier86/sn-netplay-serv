//! Tests for HTTP route contracts.
//!
//! These cover auth ordering, create-room descriptors, rate limits, and
//! internal admin observability without growing the production route module.

use super::{build_router, trace_request_path};
use crate::auth::{
    AuthError, ClientKind, LicenseAuthority, ProtectedClientAuthProof, VerifiedLicense,
};
use crate::file_relay::DisabledFileRelayBroker;
use crate::http::services::{AppServices, FileRelayPolicy};
use crate::http::{AdminAuthorizer, AppServiceDependencies};
use crate::limits::MAX_CREATE_ROOM_BODY_BYTES;
use crate::lobbies::InMemoryLobbyRegistry;
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
                auth.installation_id.as_str(),
                match auth.client_kind {
                    ClientKind::Desktop => "premium",
                    ClientKind::Android => "authenticated",
                    ClientKind::Ios => "authenticated",
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

#[test]
fn http_trace_path_excludes_capability_query_values() {
    let request = Request::builder()
        .uri("/v1/ws/input?resumeToken=secret&inputSocketToken=also-secret")
        .body(Body::empty())
        .expect("request");

    assert_eq!(trace_request_path(&request), "/v1/ws/input");
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
    let body = to_bytes(response.into_body(), 16 * 1024)
        .await
        .expect("health response body");
    let value: Value = serde_json::from_slice(&body).expect("health response json");
    assert_eq!(value["status"], "ok");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["minSupportedProtocolVersion"], 4);
    assert_eq!(value["maxSupportedProtocolVersion"], 5);
    assert!(
        value["buildSha"]
            .as_str()
            .is_some_and(|sha| !sha.is_empty())
    );
    assert!(
        value["imageIdentity"]
            .as_str()
            .is_some_and(|identity| !identity.is_empty())
    );
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
    assert_eq!(value["room"]["protocol"]["protocolVersion"], 4);
    assert_eq!(
        value["room"]["session"]["game"]["romSha256"],
        "a".repeat(64)
    );
    assert_eq!(value["room"]["session"]["core"]["coreId"], "dolphin");
    assert_eq!(value["room"]["session"]["hostClientKind"], "desktop");
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
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let value = serde_json::from_slice::<Value>(&body).expect("json");

    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["room"]["session"]["hostClientKind"], "android");
}

#[tokio::test]
async fn create_room_accepts_ios_client_auth() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rooms")
                .header("authorization", "Bearer valid")
                .header("x-client-kind", "ios")
                .header("x-installation-id", "ios-install-1")
                .header("x-req-ts", "1784069961000")
                .header("x-req-nonce", "nonce")
                .header("x-app-attest-key-id", "app-attest-key")
                .header("x-app-attest-assertion", "app-attest-assertion")
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
    assert_eq!(value["room"]["session"]["hostClientKind"], "ios");
}

#[tokio::test]
async fn create_lobby_returns_invite_player_slot_and_initial_game() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/lobbies")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "host-install")
                .body(Body::from(create_lobby_body()))
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
    assert_eq!(value["lobby"]["inviteCode"], "AB23-CD");
    assert_eq!(value["playerIndex"], 0);
    assert!(
        value["resumeToken"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    );
    assert_eq!(value["lobby"]["players"][0]["role"], "host");
    assert_eq!(value["lobby"]["players"][0]["color"], "cyan");
    assert_eq!(
        value["lobby"]["selectedGame"]["game"]["title"],
        "Starlight Ruins"
    );
    assert_eq!(value["lobby"]["visibility"], "private");
}

#[tokio::test]
async fn public_lobbies_require_auth_and_return_safe_summaries() {
    let app = app();
    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/lobbies/public")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/lobbies")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "host-install")
                .body(Body::from(create_public_lobby_body()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(create_response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/lobbies/public")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "browser-install")
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
    assert_eq!(value["lobbies"].as_array().expect("lobbies").len(), 1);
    assert_eq!(value["lobbies"][0]["inviteCode"], "AB23-CD");
    assert_eq!(value["lobbies"][0]["visibility"], "public");
    assert_eq!(value["lobbies"][0]["status"], "gameSelected");
    assert_eq!(value["lobbies"][0]["hostedBy"], "Host");
    assert_eq!(value["lobbies"][0]["playerCount"], 1);
    assert_eq!(value["lobbies"][0]["maxPlayers"], 2);
    assert_eq!(value["lobbies"][0]["openSlots"], 1);
    assert_eq!(
        value["lobbies"][0]["selectedGame"]["title"],
        "Starlight Ruins"
    );
    assert!(
        value["lobbies"][0]["selectedGame"]
            .get("contentSha256")
            .is_none()
    );
    assert!(
        value["lobbies"][0]["selectedGame"]
            .get("romSizeBytes")
            .is_none()
    );
}

#[tokio::test]
async fn join_lobby_assigns_second_player_and_status_hides_resume_token() {
    let app = app();
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/lobbies")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "host-install")
                .body(Body::from(create_lobby_body()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(create_response.status(), StatusCode::OK);

    let join_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/lobbies/AB23-CD/join")
                .header("authorization", "Bearer valid")
                .header("x-install-id", "guest-install")
                .body(Body::from(join_lobby_body()))
                .expect("request"),
        )
        .await
        .expect("response");
    let join_status = join_response.status();
    let join_body = to_bytes(join_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let join_value = serde_json::from_slice::<Value>(&join_body).expect("json");

    assert_eq!(join_status, StatusCode::OK);
    assert_eq!(join_value["playerIndex"], 1);
    assert_eq!(join_value["lobby"]["players"][1]["color"], "violet");

    let status_response = app
        .oneshot(
            Request::builder()
                .uri("/v1/lobbies/AB23-CD/status")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status_body = to_bytes(status_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let status_value = serde_json::from_slice::<Value>(&status_body).expect("json");

    assert_eq!(status_value["lobby"]["players"][1]["displayNumber"], 2);
    assert!(status_value.get("resumeToken").is_none());
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
async fn create_room_rejects_link_cable_system_mismatch() {
    let mut body = create_room_value();
    body["session"]["mode"] = json!("linkCable");
    body["session"]["link"] = json!({
        "systemFamily": "gba",
        "linkProtocol": "gba-link-cable-v1",
        "runtimeProfile": "mgba-link-runtime-v1",
        "maxPlayers": 2,
        "transport": "relay"
    });

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
async fn websocket_join_rejects_partial_reconnect_query() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/v1/ws?inviteCode=AB23-CD&role=guest&protocolVersion=4&playerIndex=1")
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
    let services = AppServices::new(AppServiceDependencies {
        license_authority: Arc::new(FakeLicenseAuthority),
        rooms: Arc::new(InMemoryRoomRegistry::new(Arc::new(
            StaticInviteCodeGenerator,
        ))),
        lobbies: Arc::new(InMemoryLobbyRegistry::new(Arc::new(
            StaticInviteCodeGenerator,
        ))),
        file_relay: Arc::new(DisabledFileRelayBroker),
        file_relay_policy: FileRelayPolicy {
            save_states_enabled: false,
            temporary_roms_enabled: false,
            temporary_rom_max_bytes: 104_857_600,
            direct_roms_enabled: false,
            direct_rom_allowed_systems: Vec::new(),
        },
        rate_limiter: Arc::new(InMemoryRateLimiter::new(RateLimitPolicy {
            create_room_per_minute,
            websocket_join_per_minute: 30,
            room_status_per_minute: 120,
        })),
        metrics: Arc::new(InMemoryMetrics::new()),
        protocol_rollout: crate::protocol::NetplayProtocolRolloutPolicy::default(),
        admin_authorizer: AdminAuthorizer::new(Some("admin-token".to_string())),
        trust_proxy_headers: false,
    });

    build_router(services)
}

fn create_room_body() -> String {
    create_room_value().to_string()
}

fn create_lobby_body() -> String {
    json!({
        "protocolVersion": 4,
        "displayName": "Host",
        "capabilities": {
            "supportsLobby": true,
            "supportsTemporarySessionRomRelay": true,
            "supportsLobbyVoice": true,
            "supportsMultiGameLobby": true
        },
        "initialGame": {
            "title": "Starlight Ruins",
            "systemId": "snes",
            "coreId": "snes9x",
            "contentSha256": "c".repeat(64),
            "romSizeBytes": 2097152,
            "startStateLabel": "fresh"
        }
    })
    .to_string()
}

fn create_public_lobby_body() -> String {
    let mut value = serde_json::from_str::<Value>(&create_lobby_body()).expect("lobby body");

    value["visibility"] = json!("public");

    value.to_string()
}

fn join_lobby_body() -> String {
    json!({
        "protocolVersion": 4,
        "displayName": "Guest",
        "capabilities": {
            "supportsLobby": true,
            "supportsTemporarySessionRomRelay": false,
            "supportsLobbyVoice": false,
            "supportsMultiGameLobby": false
        }
    })
    .to_string()
}

fn create_room_value() -> Value {
    json!({
        "desktopProtocolVersion": 4,
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
                "coreOptionsSha256": "b".repeat(64),
                "stateFormat": "dolphin:gamecube:libretro-serialize-v1"
            },
            "controller": {
                "inputDelayFrames": 3
            }
        }
    })
}
