#![allow(dead_code)]

mod websocket_messages;

use async_trait::async_trait;
use reqwest::Client;
use sb_netplay_serv::auth::{
    AuthError, ClientKind, LicenseAuthority, ProtectedClientAuthProof, VerifiedLicense,
};
use sb_netplay_serv::file_relay::DisabledFileRelayBroker;
use sb_netplay_serv::http::{
    AdminAuthorizer, AppServiceDependencies, AppServices, FileRelayPolicy, LinkCableRolloutPolicy,
    build_router,
};
use sb_netplay_serv::lobbies::InMemoryLobbyRegistry;
use sb_netplay_serv::observability::{InMemoryMetrics, MetricsRecorder, MetricsSnapshot};
use sb_netplay_serv::protocol::{LEGACY_NETPLAY_PROTOCOL_VERSION, NETPLAY_PROTOCOL_VERSION};
use sb_netplay_serv::rate_limit::{InMemoryRateLimiter, RateLimitPolicy};
use sb_netplay_serv::rooms::{
    InMemoryRoomRegistry, InviteCode, InviteCodeGenerator, spawn_room_frame_clock_task,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const INVITE_CODE: &str = "AB23-CD";

pub struct SmokeServer {
    auth_calls: Arc<AtomicUsize>,
    metrics: Arc<InMemoryMetrics>,
    pub http_base: String,
    pub ws_base: String,
    frame_clock_task: JoinHandle<()>,
    task: JoinHandle<()>,
}

impl SmokeServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let address = listener.local_addr().expect("server local address");
        let rooms = Arc::new(InMemoryRoomRegistry::new(Arc::new(
            StaticInviteCodeGenerator,
        )));
        let auth_calls = Arc::new(AtomicUsize::new(0));
        let metrics = Arc::new(InMemoryMetrics::new());
        let frame_clock_task = spawn_room_frame_clock_task(rooms.clone());
        let app = build_router(test_services(rooms, auth_calls.clone(), metrics.clone()));
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .expect("serve test router");
        });

        Self {
            auth_calls,
            metrics,
            frame_clock_task,
            http_base: format!("http://{address}"),
            ws_base: format!("ws://{address}"),
            task,
        }
    }

    pub async fn create_room(&self) -> String {
        self.create_room_from_body(create_room_body()).await
    }

    pub async fn create_link_room(&self) -> String {
        self.create_room_from_body(create_link_room_body()).await
    }

    pub async fn create_v5_room(&self) -> String {
        self.create_room_from_body(create_v5_room_body()).await
    }

    pub fn auth_call_count(&self) -> usize {
        self.auth_calls.load(Ordering::SeqCst)
    }

    pub fn metrics(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    pub async fn room_status(&self) -> Value {
        Client::new()
            .get(format!("{}/v1/rooms/{INVITE_CODE}/status", self.http_base))
            .send()
            .await
            .expect("room status response")
            .json::<Value>()
            .await
            .expect("room status body")
    }

    async fn create_room_from_body(&self, body: Value) -> String {
        let response = Client::new()
            .post(format!("{}/v1/rooms", self.http_base))
            .bearer_auth("host-token")
            .header("x-install-id", "host-install")
            .json(&body)
            .send()
            .await
            .expect("create room response");
        let status = response.status();
        let body = response.json::<Value>().await.expect("create room body");

        assert_eq!(status.as_u16(), 200, "{body}");

        body["room"]["inviteCode"]
            .as_str()
            .expect("invite code")
            .to_string()
    }
}

impl Drop for SmokeServer {
    fn drop(&mut self) {
        self.frame_clock_task.abort();
        self.task.abort();
    }
}

pub struct SmokeClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    input_socket: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    player_index: Option<u8>,
    room_epoch: u64,
    session_epoch: u64,
    protocol_version: u16,
    accepts_link_grants: bool,
    link_grant: Option<SmokeLinkCableGrant>,
    next_link_sender_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmokeLinkCableGrant {
    pub contract_version: u16,
    pub room_scope: String,
    pub room_epoch: u64,
    pub session_epoch: u64,
    pub cable_epoch: u64,
    pub local_slot: u8,
    pub link_protocol: String,
    pub maximum_event_bytes: u16,
    pub queue_capacity: u16,
    pub status: String,
}

impl SmokeClient {
    pub async fn connect(server: &SmokeServer, role: &str, token: &str, install_id: &str) -> Self {
        Self::connect_initial(
            server,
            role,
            token,
            install_id,
            LEGACY_NETPLAY_PROTOCOL_VERSION,
            false,
            false,
        )
        .await
    }

    pub async fn connect_link(
        server: &SmokeServer,
        role: &str,
        token: &str,
        install_id: &str,
    ) -> Self {
        Self::connect_initial(
            server,
            role,
            token,
            install_id,
            LEGACY_NETPLAY_PROTOCOL_VERSION,
            false,
            true,
        )
        .await
    }

    pub async fn connect_v5(
        server: &SmokeServer,
        role: &str,
        token: &str,
        install_id: &str,
    ) -> Self {
        Self::connect_initial(
            server,
            role,
            token,
            install_id,
            NETPLAY_PROTOCOL_VERSION,
            false,
            false,
        )
        .await
    }

    pub async fn connect_runner_handoff(
        server: &SmokeServer,
        role: &str,
        token: &str,
        install_id: &str,
    ) -> Self {
        Self::connect_initial(
            server,
            role,
            token,
            install_id,
            LEGACY_NETPLAY_PROTOCOL_VERSION,
            true,
            false,
        )
        .await
    }

    async fn connect_initial(
        server: &SmokeServer,
        role: &str,
        token: &str,
        install_id: &str,
        protocol_version: u16,
        runner_handoff: bool,
        supports_link_contract: bool,
    ) -> Self {
        let runner_handoff_query = if runner_handoff {
            "&runnerHandoff=true"
        } else {
            ""
        };
        let capability_query = if protocol_version >= NETPLAY_PROTOCOL_VERSION {
            "&supportsScheduledStart=true&supportsClockSync=true&supportsFastInputRelay=true"
        } else {
            ""
        };
        let link_contract_query = if supports_link_contract {
            "&linkContractVersion=1"
        } else {
            ""
        };
        let mut request = format!(
            "{}/v1/ws?inviteCode={}&role={role}&protocolVersion={}{}{}{}",
            server.ws_base,
            INVITE_CODE,
            protocol_version,
            runner_handoff_query,
            capability_query,
            link_contract_query
        )
        .into_client_request()
        .expect("websocket request");
        let headers = request.headers_mut();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).expect("authorization header"),
        );
        headers.insert(
            "x-install-id",
            HeaderValue::from_str(install_id).expect("install id header"),
        );

        let (socket, response) = connect_async(request).await.expect("websocket connect");
        assert_eq!(response.status().as_u16(), 101);

        Self {
            socket,
            input_socket: None,
            player_index: None,
            room_epoch: 1,
            session_epoch: 1,
            protocol_version,
            accepts_link_grants: supports_link_contract,
            link_grant: None,
            next_link_sender_sequence: 0,
        }
    }

    pub async fn resume_from_handoff(server: &SmokeServer, handoff: &Value) -> Self {
        let player_index = handoff["yourPlayerIndex"]
            .as_u64()
            .expect("handoff player index") as u8;
        let room_epoch = handoff["roomEpoch"].as_u64().expect("handoff room epoch");
        let session_epoch = handoff["sessionEpoch"]
            .as_u64()
            .expect("handoff session epoch");
        let resume_token = handoff["resumeToken"]
            .as_str()
            .expect("handoff resume token");
        let request = format!(
            "{}/v1/ws?inviteCode={}&protocolVersion={}&playerIndex={player_index}&roomEpoch={room_epoch}&resumeToken={resume_token}",
            server.ws_base, INVITE_CODE, LEGACY_NETPLAY_PROTOCOL_VERSION
        )
        .into_client_request()
        .expect("resume websocket request");
        let (socket, response) = connect_async(request).await.expect("runner resume");
        assert_eq!(response.status().as_u16(), 101);

        Self {
            socket,
            input_socket: None,
            player_index: Some(player_index),
            room_epoch,
            session_epoch,
            protocol_version: LEGACY_NETPLAY_PROTOCOL_VERSION,
            accepts_link_grants: false,
            link_grant: None,
            next_link_sender_sequence: 0,
        }
    }

    pub async fn close_control(mut self) {
        self.socket.close(None).await.expect("close control socket");
    }

    pub async fn connect_input(
        &mut self,
        server: &SmokeServer,
        token: &str,
        install_id: &str,
        room_joined: &Value,
    ) {
        self.connect_input_with_auth(server, Some((token, install_id)), room_joined)
            .await;
    }

    pub async fn connect_input_capability(&mut self, server: &SmokeServer, room_joined: &Value) {
        self.connect_input_with_auth(server, None, room_joined)
            .await;
    }

    async fn connect_input_with_auth(
        &mut self,
        server: &SmokeServer,
        auth: Option<(&str, &str)>,
        room_joined: &Value,
    ) {
        let player_index = room_joined["yourPlayerIndex"]
            .as_u64()
            .expect("joined player index") as u8;
        let input_socket_token = room_joined["inputSocketToken"]
            .as_str()
            .expect("input socket token");
        let mut request = format!(
            "{}/v1/ws/input?inviteCode={}&protocolVersion={}&playerIndex={player_index}&roomEpoch={}&sessionEpoch={}&inputSocketToken={}",
            server.ws_base,
            INVITE_CODE,
            self.protocol_version,
            self.room_epoch,
            self.session_epoch,
            input_socket_token
        )
        .into_client_request()
        .expect("input websocket request");
        request.headers_mut().insert(
            "sec-websocket-extensions",
            HeaderValue::from_static("permessage-deflate"),
        );
        if let Some((token, install_id)) = auth {
            let headers = request.headers_mut();
            headers.insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {token}")).expect("authorization header"),
            );
            headers.insert(
                "x-install-id",
                HeaderValue::from_str(install_id).expect("install id header"),
            );
        }

        let (socket, response) = connect_async(request)
            .await
            .expect("input websocket connect");
        assert_eq!(response.status().as_u16(), 101);
        assert!(response.headers().get("sec-websocket-extensions").is_none());
        self.input_socket = Some(socket);
        self.player_index = Some(player_index);
    }

    pub fn epochs(&self) -> (u64, u64) {
        (self.room_epoch, self.session_epoch)
    }

    pub fn link_grant(&self) -> Option<&SmokeLinkCableGrant> {
        self.link_grant.as_ref()
    }
}

pub fn compatibility_fingerprint() -> Value {
    json!({
        "desktopVersion": "0.2.13",
        "protocolVersion": LEGACY_NETPLAY_PROTOCOL_VERSION,
        "systemId": "gamecube",
        "coreId": "dolphin",
        "coreBuild": "5.0-netplay",
        "contentHash": "a".repeat(64),
        "settingsHash": "b".repeat(64),
        "cheatsHash": "c".repeat(64),
        "systemDataHash": null,
        "saveDataMode": "netplay"
    })
}

pub fn v5_compatibility_fingerprint(digest_mode: &str) -> Value {
    json!({
        "desktopVersion": "2.1.0-test",
        "protocolVersion": NETPLAY_PROTOCOL_VERSION,
        "systemId": "snes",
        "coreId": "snes9x",
        "coreBuild": "android-test-artifact",
        "stateFormat": "snes9x:snes:libretro-serialize-v1",
        "contentHash": "a".repeat(64),
        "settingsHash": empty_sha256(),
        "cheatsHash": empty_sha256(),
        "systemDataHash": null,
        "saveDataMode": "netplay",
        "determinismV5": {
            "netplayCoreCompatibilityId": "snes9x-2025-compat-v1",
            "localArtifactId": "android-test-artifact",
            "platformClass": "libretro-arm64-le-v1",
            "coreOptionsDigest": empty_sha256(),
            "controllerProfileIds": ["retropad-port-1-v1"],
            "inputCodecId": "shadowboy-retropad-v1-le",
            "inputPayloadSize": 10,
            "predictorId": "shadowboy-retropad-predictor-v1",
            "nominalFrameRateNumerator": 150247,
            "nominalFrameRateDenominator": 2500,
            "romSizeBytes": 1024,
            "contentTransformationDigest": null,
            "startupStatePolicyId": "load-start-frame-state-v1",
            "replayOutputSuppressed": true,
            "digestMode": digest_mode,
            "digestAlgorithmId": if digest_mode == "disabled" {
                Value::Null
            } else {
                json!("sha256-libretro-serialize-start-frame-v1")
            }
        }
    })
}

pub async fn connect_ready_pair(server: &SmokeServer) -> (SmokeClient, SmokeClient) {
    let mut host = SmokeClient::connect(server, "host", "host-token", "host-install").await;
    let mut guest = SmokeClient::connect(server, "guest", "guest-token", "guest-install").await;

    let host_join = host.expect_type("roomJoined").await;
    let guest_join = guest.expect_type("roomJoined").await;
    assert_eq!(host_join["yourPlayerIndex"], 0);
    assert_eq!(guest_join["yourPlayerIndex"], 1);
    assert!(host_join.get("linkCableGrant").is_none());
    assert!(guest_join.get("linkCableGrant").is_none());
    assert!(host.link_grant().is_none());
    assert!(guest.link_grant().is_none());
    host.expect_type("compatibilityRequested").await;
    host.connect_input(server, "host-token", "host-install", &host_join)
        .await;
    guest
        .connect_input(server, "guest-token", "guest-install", &guest_join)
        .await;

    (host, guest)
}

pub async fn connect_link_pair(server: &SmokeServer) -> (SmokeClient, SmokeClient) {
    let mut host = SmokeClient::connect_link(server, "host", "host-token", "host-install").await;
    let mut guest =
        SmokeClient::connect_link(server, "guest", "guest-token", "guest-install").await;

    let host_join = host.expect_type("roomJoined").await;
    let guest_join = guest.expect_type("roomJoined").await;
    assert_eq!(host_join["yourPlayerIndex"], 0);
    assert_eq!(guest_join["yourPlayerIndex"], 1);
    assert!(host_join.get("inputSocketToken").is_none());
    assert!(guest_join.get("inputSocketToken").is_none());
    host.expect_type("compatibilityRequested").await;

    let host_grant = host.expect_link_grant_status("ready").await;
    let guest_grant = guest.expect_link_grant_status("ready").await;
    assert!(
        host_grant
            .room_scope
            .parse::<u64>()
            .is_ok_and(|room_scope| room_scope > 0)
    );
    assert_eq!(guest_grant.room_scope, host_grant.room_scope);
    assert!(host_grant.cable_epoch > 0);
    assert_eq!(guest_grant.cable_epoch, host_grant.cable_epoch);
    assert_eq!(host_grant.local_slot, 0);
    assert_eq!(guest_grant.local_slot, 1);
    assert_eq!(host_grant.link_protocol, "gba-sio-multi-v1");
    assert_eq!(guest_grant.link_protocol, host_grant.link_protocol);
    assert!(host_grant.queue_capacity > 0);
    assert_eq!(guest_grant.queue_capacity, host_grant.queue_capacity);

    (host, guest)
}

pub async fn connect_v5_pair(server: &SmokeServer) -> (SmokeClient, SmokeClient) {
    let mut host = SmokeClient::connect_v5(server, "host", "host-token", "host-install").await;
    let mut guest = SmokeClient::connect_v5(server, "guest", "guest-token", "guest-install").await;

    let host_join = host.expect_type("roomJoined").await;
    let guest_join = guest.expect_type("roomJoined").await;
    assert_eq!(host_join["room"]["protocol"]["roomProtocolVersion"], 5);
    assert_eq!(guest_join["room"]["protocol"]["roomProtocolVersion"], 5);
    host.expect_type("compatibilityRequested").await;
    host.connect_input(server, "host-token", "host-install", &host_join)
        .await;
    guest
        .connect_input(server, "guest-token", "guest-install", &guest_join)
        .await;

    (host, guest)
}

pub async fn move_pair_to_syncing(host: &mut SmokeClient, guest: &mut SmokeClient) {
    host.send(json!({
        "type": "setCompatibilityFingerprint",
        "fingerprint": compatibility_fingerprint()
    }))
    .await;
    guest
        .send(json!({
            "type": "setCompatibilityFingerprint",
            "fingerprint": compatibility_fingerprint()
        }))
        .await;

    let host_state = host.expect_room_status("syncingState").await;
    let guest_state = guest.expect_room_status("syncingState").await;
    assert_eq!(host_state["room"]["status"], "syncingState");
    assert_eq!(guest_state["room"]["status"], "syncingState");
}

pub async fn move_v5_pair_to_syncing(
    host: &mut SmokeClient,
    guest: &mut SmokeClient,
    digest_mode: &str,
) {
    let fingerprint = v5_compatibility_fingerprint(digest_mode);
    host.send(json!({
        "type": "setCompatibilityFingerprint",
        "fingerprint": fingerprint.clone()
    }))
    .await;
    guest
        .send(json!({
            "type": "setCompatibilityFingerprint",
            "fingerprint": fingerprint
        }))
        .await;

    host.expect_room_status("syncingState").await;
    guest.expect_room_status("syncingState").await;
}

pub fn link_cable_compatibility() -> Value {
    json!({
        "protocolVersion": LEGACY_NETPLAY_PROTOCOL_VERSION,
        "systemFamily": "gba",
        "linkProtocol": "gba-sio-multi-v1",
        "runtimeProfile": "mgba-link-runtime-v1",
        "coreBuildId": "android-mgba-0.10.5-sb1",
        "supportedModes": ["multi"]
    })
}

pub async fn move_link_pair_to_syncing(host: &mut SmokeClient, guest: &mut SmokeClient) {
    host.send(json!({
        "type": "setLinkCableCompatibility",
        "compatibility": link_cable_compatibility()
    }))
    .await;
    guest
        .send(json!({
            "type": "setLinkCableCompatibility",
            "compatibility": link_cable_compatibility()
        }))
        .await;

    let host_state = host.expect_room_status("syncingState").await;
    let guest_state = guest.expect_room_status("syncingState").await;
    assert_eq!(host_state["room"]["status"], "syncingState");
    assert_eq!(guest_state["room"]["status"], "syncingState");
}

pub fn snapshot_payload(bytes: &[u8]) -> (Value, Value) {
    snapshot_payload_at(bytes, 0)
}

pub fn snapshot_payload_at(bytes: &[u8], repair_frame: u64) -> (Value, Value) {
    (
        json!({
            "type": "snapshotChunk",
            "chunk": {
                "snapshotId": "snapshot-1",
                "repairFrame": repair_frame,
                "index": 0,
                "bytes": bytes
            }
        }),
        json!({
            "type": "snapshotComplete",
            "manifest": {
                "snapshotId": "snapshot-1",
                "repairFrame": repair_frame,
                "totalBytes": bytes.len(),
                "sha256": format!("{:x}", Sha256::digest(bytes))
            }
        }),
    )
}

impl SmokeClient {
    async fn expect_room_status(&mut self, status: &str) -> Value {
        loop {
            let message = self.next_json().await;
            if message["type"] == "error" {
                panic!("unexpected netplay error while waiting for {status}: {message}");
            }
            if message["type"] == "roomStateChanged" && message["room"]["status"] == status {
                return message;
            }
        }
    }
}

fn test_services(
    rooms: Arc<InMemoryRoomRegistry>,
    auth_calls: Arc<AtomicUsize>,
    metrics: Arc<InMemoryMetrics>,
) -> AppServices {
    AppServices::new(AppServiceDependencies {
        license_authority: Arc::new(FakeLicenseAuthority { auth_calls }),
        rooms,
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
            create_room_per_minute: 100,
            websocket_join_per_minute: 100,
            room_status_per_minute: 100,
        })),
        metrics,
        protocol_rollout: sb_netplay_serv::protocol::NetplayProtocolRolloutPolicy::default(),
        link_cable_rollout: LinkCableRolloutPolicy::new(true),
        admin_authorizer: AdminAuthorizer::new(None),
        trust_proxy_headers: false,
    })
}

fn create_room_body() -> Value {
    json!({
        "desktopProtocolVersion": LEGACY_NETPLAY_PROTOCOL_VERSION,
        "session": {
            "hostAppVersion": "0.2.13",
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

fn create_v5_room_body() -> Value {
    json!({
        "desktopProtocolVersion": NETPLAY_PROTOCOL_VERSION,
        "minimumProtocolVersion": NETPLAY_PROTOCOL_VERSION,
        "session": {
            "hostAppVersion": "2.1.0-test",
            "game": {
                "systemId": "snes",
                "title": "V5 Integration Fixture",
                "romSha256": "a".repeat(64),
                "contentKey": "snes-v5-integration"
            },
            "core": {
                "coreId": "snes9x",
                "coreOptionsSha256": empty_sha256(),
                "stateFormat": "snes9x:snes:libretro-serialize-v1"
            },
            "controller": { "inputDelayFrames": 3 },
            "romIdentity": {
                "system": "snes",
                "coreId": "snes9x",
                "contentHash": "a".repeat(64),
                "sizeBytes": 1024,
                "displayName": "V5 Integration Fixture"
            }
        }
    })
}

fn empty_sha256() -> String {
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()
}

fn create_link_room_body() -> Value {
    json!({
        "desktopProtocolVersion": LEGACY_NETPLAY_PROTOCOL_VERSION,
        "linkContractVersion": 1,
        "session": {
            "hostAppVersion": "0.2.13",
            "mode": "linkCable",
            "game": {
                "systemId": "gba",
                "title": "Pokemon Ruby",
                "romSha256": "a".repeat(64),
                "contentKey": "gba-ruby"
            },
            "core": {
                "coreId": "mgba",
                "coreName": "mGBA",
                "coreVersion": "link-runtime",
                "coreOptionsSha256": "b".repeat(64)
            },
            "link": {
                "systemFamily": "gba",
                "linkProtocol": "gba-sio-multi-v1",
                "runtimeProfile": "mgba-link-runtime-v1",
                "maxPlayers": 2,
                "transport": "relay"
            }
        }
    })
}

struct FakeLicenseAuthority {
    auth_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl LicenseAuthority for FakeLicenseAuthority {
    async fn verify_client_access(
        &self,
        auth: ProtectedClientAuthProof,
        _feature: &'static str,
    ) -> Result<VerifiedLicense, AuthError> {
        self.auth_calls.fetch_add(1, Ordering::SeqCst);
        let subject = match auth.access_token.expose_secret() {
            "host-token" => "host-subject",
            "guest-token" => "guest-subject",
            _ => return Err(AuthError::Unauthorized),
        };

        Ok(VerifiedLicense::with_entitlement(
            ClientKind::Desktop,
            auth.installation_id.as_str(),
            subject,
            "premium",
            vec!["netplay".to_string()],
            true,
            false,
        ))
    }
}

struct StaticInviteCodeGenerator;

impl InviteCodeGenerator for StaticInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        InviteCode::parse(INVITE_CODE).expect("static invite")
    }
}
