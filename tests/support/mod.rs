use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use sb_netplay_serv::auth::{
    AuthError, ClientKind, LicenseAuthority, ProtectedClientAuthProof, VerifiedLicense,
};
use sb_netplay_serv::http::{AdminAuthorizer, AppServices, build_router};
use sb_netplay_serv::observability::InMemoryMetrics;
use sb_netplay_serv::rate_limit::{InMemoryRateLimiter, RateLimitPolicy};
use sb_netplay_serv::rooms::{InMemoryRoomRegistry, InviteCode, InviteCodeGenerator};
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
    task: JoinHandle<()>,
}

impl SmokeServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let address = listener.local_addr().expect("server local address");
        let app = build_router(test_services());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test router");
        });

        Self {
            http_base: format!("http://{address}"),
            ws_base: format!("ws://{address}"),
            task,
        }
    }

    pub async fn create_room(&self) -> String {
        let response = Client::new()
            .post(format!("{}/v1/rooms", self.http_base))
            .bearer_auth("host-token")
            .header("x-install-id", "host-install")
            .json(&create_room_body())
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
        self.task.abort();
    }
}

pub struct SmokeClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl SmokeClient {
    pub async fn connect(server: &SmokeServer, role: &str, token: &str, install_id: &str) -> Self {
        let mut request = format!(
            "{}/v1/ws?inviteCode={}&role={role}&protocolVersion=1",
            server.ws_base, INVITE_CODE
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

        Self { socket }
    }

    pub async fn send(&mut self, payload: Value) {
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
            Message::Text(payload) => serde_json::from_str(payload.as_str()).expect("json message"),
            other => panic!("unexpected websocket message: {other:?}"),
        }
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
        loop {
            let message = self.next_json().await;
            if message["type"] == "inputFrame"
                && message["input"]["playerIndex"] == u64::from(player_index)
            {
                return message;
            }
        }
    }
}

pub fn compatibility_fingerprint() -> Value {
    json!({
        "desktopVersion": "0.2.13",
        "protocolVersion": 1,
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

pub fn snapshot_payload(bytes: &[u8]) -> (Value, Value) {
    (
        json!({
            "type": "snapshotChunk",
            "chunk": {
                "index": 0,
                "bytes": bytes
            }
        }),
        json!({
            "type": "snapshotComplete",
            "manifest": {
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
            if message["type"] == "roomStateChanged" && message["room"]["status"] == status {
                return message;
            }
        }
    }
}

fn test_services() -> AppServices {
    AppServices::new(
        Arc::new(FakeLicenseAuthority),
        Arc::new(InMemoryRoomRegistry::new(Arc::new(
            StaticInviteCodeGenerator,
        ))),
        Arc::new(InMemoryRateLimiter::new(RateLimitPolicy {
            create_room_per_minute: 100,
            websocket_join_per_minute: 100,
            room_status_per_minute: 100,
        })),
        Arc::new(InMemoryMetrics::new()),
        AdminAuthorizer::new(None),
        false,
    )
}

fn create_room_body() -> Value {
    json!({
        "desktopProtocolVersion": 1,
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
