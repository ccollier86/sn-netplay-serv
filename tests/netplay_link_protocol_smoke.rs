mod support;

use serde_json::json;
use support::{SmokeServer, connect_ready_pair, move_link_pair_to_syncing};

#[tokio::test]
async fn two_clients_start_link_room_and_exchange_link_packets() {
    let server = SmokeServer::start().await;
    let invite_code = server.create_link_room().await;
    assert_eq!(invite_code, "AB23-CD");
    let (mut host, mut guest) = connect_ready_pair(&server).await;
    move_link_pair_to_syncing(&mut host, &mut guest).await;

    host.send(json!({ "type": "ready" })).await;
    guest.send(json!({ "type": "ready" })).await;
    host.expect_type("startSession").await;
    guest.expect_type("startSession").await;

    host.send(json!({
        "type": "linkCablePacket",
        "packet": {
            "playerIndex": 0,
            "sequence": 1,
            "emulatedTime": 16,
            "payload": [1, 2, 3]
        }
    }))
    .await;
    host.expect_no_link_packet_from(0).await;
    let packet = guest.expect_link_packet_from(0).await;
    assert_eq!(packet["packet"]["payload"], json!([1, 2, 3]));

    guest
        .send(json!({
            "type": "linkCablePacket",
            "packet": {
                "playerIndex": 1,
                "sequence": 1,
                "emulatedTime": 18,
                "payload": [4, 5]
            }
        }))
        .await;
    guest.expect_no_link_packet_from(1).await;
    let packet = host.expect_link_packet_from(1).await;
    assert_eq!(packet["packet"]["payload"], json!([4, 5]));
}

#[tokio::test]
async fn link_packet_before_start_is_rejected() {
    let server = SmokeServer::start().await;
    server.create_link_room().await;
    let (mut host, mut guest) = connect_ready_pair(&server).await;
    move_link_pair_to_syncing(&mut host, &mut guest).await;

    host.send(json!({
        "type": "linkCablePacket",
        "packet": {
            "playerIndex": 0,
            "sequence": 1,
            "emulatedTime": 16,
            "payload": [1]
        }
    }))
    .await;

    host.expect_error("notPlaying").await;
}
