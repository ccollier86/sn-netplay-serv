mod support;

use serde_json::json;
use support::{
    SmokeServer, compatibility_fingerprint, connect_ready_pair, move_pair_to_syncing,
    snapshot_payload,
};

#[tokio::test]
async fn two_clients_sync_snapshot_start_and_exchange_input() {
    let server = SmokeServer::start().await;
    let invite_code = server.create_room().await;
    assert_eq!(invite_code, "AB23-CD");
    let (mut host, mut guest) = connect_ready_pair(&server).await;
    move_pair_to_syncing(&mut host, &mut guest).await;
    let (chunk, complete) = snapshot_payload(&[1, 2, 3, 4]);

    host.send(chunk).await;
    let relayed_chunk = guest.expect_type("snapshotChunk").await;
    assert!(relayed_chunk["roomEpoch"].is_number());
    assert!(relayed_chunk["sessionEpoch"].is_number());
    assert_eq!(relayed_chunk["chunk"]["bytes"], json!([1, 2, 3, 4]));

    host.send(complete).await;
    let relayed_complete = guest.expect_type("snapshotComplete").await;
    assert_eq!(relayed_complete["roomEpoch"], relayed_chunk["roomEpoch"]);
    assert_eq!(
        relayed_complete["sessionEpoch"],
        relayed_chunk["sessionEpoch"]
    );
    assert_eq!(relayed_complete["manifest"]["totalBytes"], 4);

    host.send(json!({ "type": "ready" })).await;
    guest.send(json!({ "type": "ready" })).await;
    let host_start = host.expect_type("startSession").await;
    let guest_start = guest.expect_type("startSession").await;
    assert_eq!(host_start["startFrame"], 0);
    assert_eq!(guest_start["startFrame"], 0);

    host.send_input_frame(0, vec![1, 0, 0, 0]).await;
    guest.send_input_frame(0, vec![0, 1, 0, 0]).await;

    let guest_input = guest.expect_input_from(0).await;
    assert_eq!(guest_input["input"]["playerIndex"], 0);
    assert_eq!(guest_input["input"]["payload"], json!([1, 0, 0, 0]));

    let host_input = host.expect_input_from(1).await;
    assert_eq!(host_input["input"]["playerIndex"], 1);
    assert_eq!(host_input["input"]["payload"], json!([0, 1, 0, 0]));
}

#[tokio::test]
async fn guest_snapshot_payload_is_rejected() {
    let server = SmokeServer::start().await;
    server.create_room().await;
    let (mut host, mut guest) = connect_ready_pair(&server).await;
    move_pair_to_syncing(&mut host, &mut guest).await;
    let (chunk, _) = snapshot_payload(&[7, 8, 9]);

    guest.send(chunk).await;

    guest.expect_error("hostOnly").await;
}

#[tokio::test]
async fn compatibility_mismatch_is_reported_before_sync() {
    let server = SmokeServer::start().await;
    server.create_room().await;
    let (mut host, mut guest) = connect_ready_pair(&server).await;
    let mut guest_fingerprint = compatibility_fingerprint();
    guest_fingerprint["settingsHash"] = json!("d".repeat(64));

    host.send(json!({
        "type": "setCompatibilityFingerprint",
        "fingerprint": compatibility_fingerprint()
    }))
    .await;
    guest
        .send(json!({
            "type": "setCompatibilityFingerprint",
            "fingerprint": guest_fingerprint
        }))
        .await;

    guest.expect_error("compatibilityMismatch").await;
}
