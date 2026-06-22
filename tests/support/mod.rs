#![allow(dead_code)]

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use sb_netplay_serv::auth::{
    AuthError, ClientKind, LicenseAuthority, ProtectedClientAuthProof, VerifiedLicense,
};
use sb_netplay_serv::file_relay::DisabledFileRelayBroker;
use sb_netplay_serv::http::{
    AdminAuthorizer, AppServiceDependencies, AppServices, FileRelayPolicy, build_router,
};
use sb_netplay_serv::lobbies::InMemoryLobbyRegistry;
use sb_netplay_serv::observability::InMemoryMetrics;
use sb_netplay_serv::protocol::{
    InputFrame, InputFrameBatch, NETPLAY_PROTOCOL_VERSION, decode_input_frame_batch,
    encode_input_frame_batch,
};
use sb_netplay_serv::rate_limit::{InMemoryRateLimiter, RateLimitPolicy};
use sb_netplay_serv::rooms::{
    InMemoryRoomRegistry, InviteCode, InviteCodeGenerator, PlayerIndex, spawn_room_frame_clock_task,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const READ_TIMEOUT: Duration = Duration::from_secs(2);
const INVITE_CODE: &str = "AB23-CD";

pub struct SmokeServer {
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
        let frame_clock_task = spawn_room_frame_clock_task(rooms.clone());
        let app = build_router(test_services(rooms));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test router");
        });

        Self {
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
}

impl SmokeClient {
    pub async fn connect(server: &SmokeServer, role: &str, token: &str, install_id: &str) -> Self {
        let mut request = format!(
            "{}/v1/ws?inviteCode={}&role={role}&protocolVersion={}",
            server.ws_base, INVITE_CODE, NETPLAY_PROTOCOL_VERSION
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
        }
    }

    pub async fn connect_input(
        &mut self,
        server: &SmokeServer,
        token: &str,
        install_id: &str,
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
            NETPLAY_PROTOCOL_VERSION,
            self.room_epoch,
            self.session_epoch,
            input_socket_token
        )
        .into_client_request()
        .expect("input websocket request");
        let headers = request.headers_mut();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).expect("authorization header"),
        );
        headers.insert(
            "x-install-id",
            HeaderValue::from_str(install_id).expect("install id header"),
        );

        let (socket, response) = connect_async(request)
            .await
            .expect("input websocket connect");
        assert_eq!(response.status().as_u16(), 101);
        self.input_socket = Some(socket);
        self.player_index = Some(player_index);
    }

    pub async fn send(&mut self, mut payload: Value) {
        self.attach_epochs(&mut payload);
        self.socket
            .send(Message::Text(payload.to_string().into()))
            .await
            .expect("send websocket message");
    }

    pub async fn next_json(&mut self) -> Value {
        let message = timeout(READ_TIMEOUT, self.socket.next())
            .await
            .expect("websocket message timed out")
            .expect("websocket message")
            .expect("websocket result");

        match message {
            Message::Text(payload) => {
                let value = serde_json::from_str(payload.as_str()).expect("json message");
                self.update_epochs(&value);
                value
            }
            other => panic!("unexpected websocket message: {other:?}"),
        }
    }

    pub async fn send_input_frame(&mut self, frame: u64, payload: Vec<u8>) {
        let player_index = PlayerIndex::new(
            self.player_index.expect("connected player index"),
            sb_netplay_serv::limits::MVP_ROOM_CAPACITY,
        )
        .expect("valid player index");
        let encoded = encode_input_frame_batch(&InputFrameBatch {
            frames: vec![InputFrame {
                frame,
                payload,
                player_index,
            }],
            player_index,
            room_epoch: self.room_epoch,
            session_epoch: self.session_epoch,
        })
        .expect("encoded input batch");
        let input_socket = self.input_socket.as_mut().expect("input socket connected");

        input_socket
            .send(Message::Binary(encoded.into()))
            .await
            .expect("send input batch");
    }

    pub async fn expect_type(&mut self, message_type: &str) -> Value {
        loop {
            let message = self.next_json().await;
            if message["type"] == message_type {
                return message;
            }
        }
    }

    pub async fn expect_error(&mut self, code: &str) -> Value {
        loop {
            let message = self.next_json().await;
            if message["type"] == "error" && message["code"] == code {
                return message;
            }
        }
    }

    pub async fn expect_input_from(&mut self, player_index: u8) -> Value {
        let input_socket = self.input_socket.as_mut().expect("input socket connected");

        loop {
            let message = timeout(READ_TIMEOUT, input_socket.next())
                .await
                .expect("input websocket message timed out")
                .expect("input websocket message")
                .expect("input websocket result");

            if let Message::Binary(payload) = message {
                let Ok(batch) = decode_input_frame_batch(&payload) else {
                    continue;
                };
                if batch.player_index.zero_based() != player_index {
                    continue;
                }
                let input = batch.frames.first().expect("input frame");

                return json!({
                    "type": "inputFrame",
                    "input": {
                        "playerIndex": input.player_index.zero_based(),
                        "frame": input.frame,
                        "payload": input.payload
                    }
                });
            }
        }
    }

    pub async fn expect_link_packet_from(&mut self, player_index: u8) -> Value {
        loop {
            let message = self.next_json().await;
            if message["type"] == "linkCablePacket"
                && message["packet"]["playerIndex"] == u64::from(player_index)
            {
                return message;
            }
        }
    }

    pub async fn expect_no_link_packet_from(&mut self, player_index: u8) {
        let result = timeout(Duration::from_millis(200), async {
            loop {
                let message = self.next_json().await;
                if message["type"] == "linkCablePacket"
                    && message["packet"]["playerIndex"] == u64::from(player_index)
                {
                    return message;
                }
            }
        })
        .await;

        if let Ok(message) = result {
            panic!("unexpected echoed link packet: {message}");
        }
    }

    fn attach_epochs(&self, payload: &mut Value) {
        let Some(object) = payload.as_object_mut() else {
            return;
        };
        let Some(message_type) = object.get("type").and_then(Value::as_str) else {
            return;
        };

        if message_type == "ping" {
            return;
        }

        object
            .entry("roomEpoch")
            .or_insert_with(|| json!(self.room_epoch));
        object
            .entry("sessionEpoch")
            .or_insert_with(|| json!(self.session_epoch));
    }

    fn update_epochs(&mut self, message: &Value) {
        if let Some(room_epoch) = message["roomEpoch"].as_u64() {
            self.room_epoch = room_epoch;
        } else if let Some(room_epoch) = message["room"]["roomEpoch"].as_u64() {
            self.room_epoch = room_epoch;
        }

        if let Some(session_epoch) = message["sessionEpoch"].as_u64() {
            self.session_epoch = session_epoch;
        } else if let Some(session_epoch) = message["room"]["sessionEpoch"].as_u64() {
            self.session_epoch = session_epoch;
        }

        if message["type"] == "roomJoined" {
            self.player_index = message["yourPlayerIndex"].as_u64().map(|value| value as u8);
        }
    }
}

pub fn compatibility_fingerprint() -> Value {
    json!({
        "desktopVersion": "0.2.13",
        "protocolVersion": NETPLAY_PROTOCOL_VERSION,
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

pub async fn connect_ready_pair(server: &SmokeServer) -> (SmokeClient, SmokeClient) {
    let mut host = SmokeClient::connect(server, "host", "host-token", "host-install").await;
    let mut guest = SmokeClient::connect(server, "guest", "guest-token", "guest-install").await;

    let host_join = host.expect_type("roomJoined").await;
    let guest_join = guest.expect_type("roomJoined").await;
    assert_eq!(host_join["yourPlayerIndex"], 0);
    assert_eq!(guest_join["yourPlayerIndex"], 1);
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

pub fn link_cable_compatibility() -> Value {
    json!({
        "protocolVersion": NETPLAY_PROTOCOL_VERSION,
        "systemFamily": "gba",
        "linkProtocol": "gba-link-cable-v1",
        "runtimeProfile": "mgba-link-runtime-v1",
        "systemDataHash": null
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
    (
        json!({
            "type": "snapshotChunk",
            "chunk": {
                "snapshotId": "snapshot-1",
                "repairFrame": 0,
                "index": 0,
                "bytes": bytes
            }
        }),
        json!({
            "type": "snapshotComplete",
            "manifest": {
                "snapshotId": "snapshot-1",
                "repairFrame": 0,
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

fn test_services(rooms: Arc<InMemoryRoomRegistry>) -> AppServices {
    AppServices::new(AppServiceDependencies {
        license_authority: Arc::new(FakeLicenseAuthority),
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
        metrics: Arc::new(InMemoryMetrics::new()),
        admin_authorizer: AdminAuthorizer::new(None),
        trust_proxy_headers: false,
    })
}

fn create_room_body() -> Value {
    json!({
        "desktopProtocolVersion": NETPLAY_PROTOCOL_VERSION,
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

fn create_link_room_body() -> Value {
    json!({
        "desktopProtocolVersion": NETPLAY_PROTOCOL_VERSION,
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
                "linkProtocol": "gba-link-cable-v1",
                "runtimeProfile": "mgba-link-runtime-v1",
                "maxPlayers": 2,
                "transport": "relay"
            }
        }
    })
}

struct FakeLicenseAuthority;

#[async_trait]
impl LicenseAuthority for FakeLicenseAuthority {
    async fn verify_client_access(
        &self,
        auth: ProtectedClientAuthProof,
        _feature: &'static str,
    ) -> Result<VerifiedLicense, AuthError> {
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
