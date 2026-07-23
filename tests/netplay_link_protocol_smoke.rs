mod support;

use sb_netplay_serv::protocol::{
    GbaSioMultiEvent, GbaSioMultiFrame, LinkCableAbortReason, decode_gba_sio_multi_frame,
};
use serde_json::Value;
use serde_json::json;
use support::{SmokeClient, SmokeServer, connect_link_pair, move_link_pair_to_syncing};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

#[tokio::test]
async fn two_clients_start_link_room_and_exchange_link_packets() {
    let server = SmokeServer::start().await;
    let invite_code = server.create_link_room().await;
    assert_eq!(invite_code, "AB23-CD");
    let (mut host, mut guest) = connect_link_pair(&server).await;
    move_link_pair_to_syncing(&mut host, &mut guest).await;

    host.send(json!({ "type": "ready" })).await;
    guest.send(json!({ "type": "ready" })).await;
    host.expect_type("startSession").await;
    guest.expect_type("startSession").await;

    let expected = host.send_gba_mode_set(16).await;
    host.expect_no_link_packet_from(0).await;
    let packet = guest.expect_link_packet_from(0).await;
    assert_received_mode_set(&packet, &expected);

    let expected = guest.send_gba_mode_set(18).await;
    guest.expect_no_link_packet_from(1).await;
    let packet = host.expect_link_packet_from(1).await;
    assert_received_mode_set(&packet, &expected);
}

#[tokio::test]
async fn terminal_gba_abort_packet_precedes_aborted_grants_on_both_sockets() {
    let server = SmokeServer::start().await;
    server.create_link_room().await;
    let (mut host, mut guest) = connect_link_pair(&server).await;
    move_link_pair_to_syncing(&mut host, &mut guest).await;

    host.send(json!({ "type": "ready" })).await;
    guest.send(json!({ "type": "ready" })).await;
    host.expect_type("startSession").await;
    guest.expect_type("startSession").await;

    let host_mode = host.send_gba_mode_set(16).await;
    let packet = guest.expect_link_packet_from(0).await;
    assert_received_gba_frame(&packet, &host_mode, 16);
    let guest_mode = guest.send_gba_mode_set(18).await;
    let packet = host.expect_link_packet_from(1).await;
    assert_received_gba_frame(&packet, &guest_mode, 18);

    let start = host
        .send_gba_event(
            GbaSioMultiEvent::TransferStart {
                transfer_id: 1,
                siocnt: 0x2000,
                parent_word: 0x1234,
                emulated_time: 20,
            },
            20,
        )
        .await;
    let packet = guest.expect_link_packet_from(0).await;
    assert_received_gba_frame(&packet, &start, 20);

    let cable_epoch = start.header.cable_epoch;
    let abort = guest
        .send_gba_event(
            GbaSioMultiEvent::TransferAbort {
                transfer_id: 1,
                reason: LinkCableAbortReason::Timeout,
            },
            0,
        )
        .await;
    let packet = host
        .expect_link_packet_before_grant_status(1, "aborted")
        .await;
    assert_received_gba_frame(&packet, &abort, 0);

    let host_aborted = host.expect_link_grant_status("aborted").await;
    let guest_aborted = guest.expect_link_grant_status("aborted").await;
    assert_eq!(host_aborted.cable_epoch, cable_epoch);
    assert_eq!(guest_aborted.cable_epoch, cable_epoch);
}

#[tokio::test]
async fn link_packet_before_start_is_rejected() {
    let server = SmokeServer::start().await;
    server.create_link_room().await;
    let (mut host, mut guest) = connect_link_pair(&server).await;
    move_link_pair_to_syncing(&mut host, &mut guest).await;

    host.send_gba_mode_set(16).await;

    host.expect_error("notPlaying").await;
}

#[tokio::test]
async fn link_initial_and_resume_websocket_admission_require_exact_contract_version() {
    let server = SmokeServer::start().await;
    server.create_link_room().await;

    for (query, expected_code) in [
        ("", "linkCableCapabilityRequired"),
        ("&linkContractVersion=2", "linkCableCapabilityUnsupported"),
    ] {
        assert_link_handshake_rejected(
            format!(
                "{}/v1/ws?inviteCode=AB23-CD&role=host&protocolVersion=4{query}",
                server.ws_base
            ),
            true,
            expected_code,
        )
        .await;
    }

    let mut host = SmokeClient::connect_link(&server, "host", "host-token", "host-install").await;
    let joined = host.expect_type("roomJoined").await;
    let player_index = joined["yourPlayerIndex"]
        .as_u64()
        .expect("link player index");
    let room_epoch = joined["roomEpoch"].as_u64().expect("link room epoch");
    let resume_token = joined["resumeToken"].as_str().expect("link resume token");

    for (query, expected_code) in [
        ("", "linkCableCapabilityRequired"),
        ("&linkContractVersion=2", "linkCableCapabilityUnsupported"),
    ] {
        assert_link_handshake_rejected(
            format!(
                "{}/v1/ws?inviteCode=AB23-CD&protocolVersion=4&playerIndex={player_index}&roomEpoch={room_epoch}&resumeToken={resume_token}{query}",
                server.ws_base
            ),
            false,
            expected_code,
        )
        .await;
    }
}

#[tokio::test]
async fn controller_initial_and_resume_websockets_do_not_require_link_contract_version() {
    let initial_server = SmokeServer::start().await;
    initial_server.create_room().await;
    let mut initial =
        SmokeClient::connect(&initial_server, "host", "host-token", "host-install").await;
    let initial_join = initial.expect_type("roomJoined").await;
    assert!(initial_join["inputSocketToken"].is_string());
    assert!(initial_join.get("linkCableGrant").is_none());

    let resume_server = SmokeServer::start().await;
    resume_server.create_room().await;
    let mut provisional =
        SmokeClient::connect_runner_handoff(&resume_server, "host", "host-token", "host-install")
            .await;
    let joined = provisional.expect_type("roomJoined").await;
    assert!(joined["inputSocketToken"].is_string());
    assert!(joined.get("linkCableGrant").is_none());
    provisional.close_control().await;

    let mut resumed = SmokeClient::resume_from_handoff(&resume_server, &joined).await;
    let resumed_join = resumed.expect_type("roomJoined").await;
    assert!(resumed_join["inputSocketToken"].is_string());
    assert!(resumed_join.get("linkCableGrant").is_none());
}

async fn assert_link_handshake_rejected(url: String, include_auth: bool, expected_code: &str) {
    let mut request = url.into_client_request().expect("link websocket request");
    if include_auth {
        request.headers_mut().insert(
            "authorization",
            HeaderValue::from_static("Bearer host-token"),
        );
        request
            .headers_mut()
            .insert("x-install-id", HeaderValue::from_static("host-install"));
    }

    let error = connect_async(request)
        .await
        .expect_err("link handshake should be rejected");
    let Error::Http(response) = error else {
        panic!("unexpected link handshake error: {error:?}");
    };
    assert_eq!(response.status().as_u16(), 400);
    let body = response
        .body()
        .as_deref()
        .expect("link handshake error response body");
    let value: Value = serde_json::from_slice(body).expect("link handshake error JSON");
    assert_eq!(value["code"], expected_code);
}

fn assert_received_mode_set(message: &Value, expected: &GbaSioMultiFrame) {
    assert_received_gba_frame(
        message,
        expected,
        match expected.event {
            GbaSioMultiEvent::ModeSet { emulated_time, .. } => emulated_time,
            _ => panic!("expected GBA MODE_SET fixture"),
        },
    );

    let payload: Vec<u8> =
        serde_json::from_value(message["packet"]["payload"].clone()).expect("SBLK payload bytes");
    let decoded = decode_gba_sio_multi_frame(&payload).expect("relayed GBA SIO MODE_SET");
    assert!(matches!(decoded.event, GbaSioMultiEvent::ModeSet { .. }));
}

fn assert_received_gba_frame(
    message: &Value,
    expected: &GbaSioMultiFrame,
    envelope_emulated_time: u64,
) {
    let payload: Vec<u8> =
        serde_json::from_value(message["packet"]["payload"].clone()).expect("SBLK payload bytes");
    let decoded = decode_gba_sio_multi_frame(&payload).expect("relayed GBA SIO event");

    assert_eq!(decoded, *expected);
    assert_eq!(decoded.header.room_epoch, expected.header.room_epoch);
    assert_eq!(decoded.header.session_epoch, expected.header.session_epoch);
    assert_eq!(decoded.header.cable_epoch, expected.header.cable_epoch);
    assert_eq!(
        decoded.header.sender_sequence,
        expected.header.sender_sequence
    );
    assert_eq!(decoded.header.sender_slot, expected.header.sender_slot);
    assert_eq!(
        message["packet"]["playerIndex"],
        json!(expected.header.sender_slot)
    );
    assert_eq!(
        message["packet"]["sequence"],
        json!(expected.header.sender_sequence)
    );

    assert_eq!(
        message["packet"]["emulatedTime"],
        json!(envelope_emulated_time)
    );
}
