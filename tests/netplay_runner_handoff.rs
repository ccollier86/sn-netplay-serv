mod support;

use futures_util::StreamExt;
use sb_netplay_serv::protocol::LEGACY_NETPLAY_PROTOCOL_VERSION;
use serde_json::Value;
use support::{SmokeClient, SmokeServer};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::{Error, Message};

#[tokio::test]
async fn protected_initial_handoff_resumes_and_attaches_input_without_runner_auth() {
    let server = SmokeServer::start().await;
    server.create_room().await;
    assert_eq!(server.auth_call_count(), 1);

    let mut provisional =
        SmokeClient::connect_runner_handoff(&server, "host", "host-token", "host-install").await;
    let handoff = provisional.expect_type("roomJoined").await;
    assert_eq!(server.auth_call_count(), 2);
    provisional.close_control().await;

    let mut runner = SmokeClient::resume_from_handoff(&server, &handoff).await;
    let resumed = runner.expect_type("roomJoined").await;
    assert_ne!(resumed["resumeToken"], handoff["resumeToken"]);
    assert_eq!(server.auth_call_count(), 2);

    let (mut resume_replay, response) = connect_async(resume_url(&server, &handoff))
        .await
        .expect("resume replay upgrade");
    assert_eq!(response.status().as_u16(), 101);
    let error = next_error(&mut resume_replay).await;
    assert_eq!(error["code"], "resumeTokenInvalid");

    runner.connect_input_capability(&server, &resumed).await;
    wait_for_input_connection(&server, 0).await;
    assert_eq!(server.auth_call_count(), 2);

    let replay_url = input_url(&server, &resumed);
    let (mut replay, response) = connect_async(replay_url)
        .await
        .expect("input replay upgrade");
    assert_eq!(response.status().as_u16(), 101);
    let error = next_error(&mut replay).await;
    assert_eq!(error["code"], "resumeTokenInvalid");
    assert_eq!(server.auth_call_count(), 2);
}

#[tokio::test]
async fn initial_and_handoff_initial_joins_still_require_protected_auth() {
    let server = SmokeServer::start().await;
    server.create_room().await;

    assert_handshake_status(
        format!(
            "{}/v1/ws?inviteCode=AB23-CD&role=host&protocolVersion={}",
            server.ws_base, LEGACY_NETPLAY_PROTOCOL_VERSION
        ),
        401,
    )
    .await;
    assert_handshake_status(
        format!(
            "{}/v1/ws?inviteCode=AB23-CD&role=host&protocolVersion={}&runnerHandoff=true",
            server.ws_base, LEGACY_NETPLAY_PROTOCOL_VERSION
        ),
        401,
    )
    .await;
}

#[tokio::test]
async fn partial_resume_without_auth_fails_before_initial_join_classification() {
    let server = SmokeServer::start().await;

    assert_handshake_status(
        format!(
            "{}/v1/ws?inviteCode=AB23-CD&protocolVersion={}&playerIndex=0",
            server.ws_base, LEGACY_NETPLAY_PROTOCOL_VERSION
        ),
        400,
    )
    .await;
    assert_handshake_status(
        format!(
            "{}/v1/ws?inviteCode=AB23-CD&protocolVersion={}&playerIndex=0&roomEpoch=1&resumeToken=token&runnerHandoff=true",
            server.ws_base, LEGACY_NETPLAY_PROTOCOL_VERSION
        ),
        400,
    )
    .await;
}

async fn wait_for_input_connection(server: &SmokeServer, player_index: usize) {
    timeout(Duration::from_secs(2), async {
        loop {
            let status = server.room_status().await;
            if status["room"]["players"][player_index]["inputConnected"] == true {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("input socket attached");
}

fn input_url(server: &SmokeServer, joined: &Value) -> String {
    format!(
        "{}/v1/ws/input?inviteCode=AB23-CD&protocolVersion={}&playerIndex={}&roomEpoch={}&sessionEpoch={}&inputSocketToken={}",
        server.ws_base,
        LEGACY_NETPLAY_PROTOCOL_VERSION,
        joined["yourPlayerIndex"].as_u64().expect("player index"),
        joined["roomEpoch"].as_u64().expect("room epoch"),
        joined["sessionEpoch"].as_u64().expect("session epoch"),
        joined["inputSocketToken"].as_str().expect("input token")
    )
}

fn resume_url(server: &SmokeServer, joined: &Value) -> String {
    format!(
        "{}/v1/ws?inviteCode=AB23-CD&protocolVersion={}&playerIndex={}&roomEpoch={}&resumeToken={}",
        server.ws_base,
        LEGACY_NETPLAY_PROTOCOL_VERSION,
        joined["yourPlayerIndex"].as_u64().expect("player index"),
        joined["roomEpoch"].as_u64().expect("room epoch"),
        joined["resumeToken"].as_str().expect("resume token")
    )
}

async fn assert_handshake_status(url: String, expected: u16) {
    let error = connect_async(url)
        .await
        .expect_err("handshake should be rejected");
    match error {
        Error::Http(response) => assert_eq!(response.status().as_u16(), expected),
        other => panic!("unexpected handshake error: {other:?}"),
    }
}

async fn next_error(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    let message = timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("error response timed out")
        .expect("error response")
        .expect("websocket result");
    let Message::Text(payload) = message else {
        panic!("expected text error, got {message:?}");
    };
    serde_json::from_str(payload.as_str()).expect("error json")
}
