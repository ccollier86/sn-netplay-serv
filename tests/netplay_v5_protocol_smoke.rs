mod support;

use sb_netplay_serv::protocol::InputCursorNackReason;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::{
    SmokeClient, SmokeServer, connect_v5_pair, move_v5_pair_to_syncing, snapshot_payload,
};
use tokio_tungstenite::connect_async;

#[tokio::test]
async fn strict_input_ack_nack_and_peer_fanout_use_the_exact_v5_lane() {
    let server = SmokeServer::start().await;
    server.create_v5_room().await;
    let (mut host, mut guest) = connect_v5_pair(&server).await;
    move_v5_pair_to_syncing(&mut host, &mut guest, "diagnostic").await;

    host.send_strict_input(0, &[1, 2]).await;
    let ack = host.expect_input_ack().await;
    assert_eq!(ack.player_index.zero_based(), 0);
    assert_eq!(ack.next_expected_frame, 2);
    let relayed = guest.expect_strict_input_from(0).await;
    assert_eq!(relayed.start_frame, 0);
    assert_eq!(relayed.payloads, vec![[1; 10], [2; 10]]);

    host.send_strict_input(4, &[4]).await;
    let nack = host.expect_input_nack().await;
    assert_eq!(nack.expected_frame, 2);
    assert_eq!(nack.received_frame, 4);
    assert_eq!(nack.reason, InputCursorNackReason::InputGap);

    host.send_strict_input(2, &[3]).await;
    assert_eq!(host.expect_input_ack().await.next_expected_frame, 3);
    assert_eq!(guest.expect_strict_input_from(0).await.start_frame, 2);
    host.send_strict_input(1, &[9, 9]).await;
    assert_eq!(host.expect_input_ack().await.next_expected_frame, 3);

    let metrics = server.metrics();
    assert_eq!(metrics.v5_input_batches_total, 4);
    assert_eq!(metrics.v5_input_frames_total, 6);
    assert_eq!(metrics.v5_input_frames_accepted_total, 3);
    assert_eq!(metrics.v5_input_frames_duplicate_total, 2);
    assert_eq!(metrics.v5_input_nacks_total, 1);
}

#[tokio::test]
async fn scheduled_host_release_and_two_phase_recovery_work_over_real_sockets() {
    let server = SmokeServer::start().await;
    server.create_v5_room().await;
    let (mut host, mut guest) = connect_v5_pair(&server).await;
    move_v5_pair_to_syncing(&mut host, &mut guest, "authoritative").await;
    relay_snapshot(&mut host, &mut guest, &[1, 2, 3, 4]).await;
    let scheduled_delay_ms = schedule_start(&mut host, &mut guest).await;

    host.send_strict_input(0, &[1]).await;
    assert_eq!(host.expect_input_ack().await.next_expected_frame, 1);
    assert_eq!(guest.expect_strict_input_from(0).await.start_frame, 0);
    guest.send_strict_input(0, &[2]).await;
    assert_eq!(guest.expect_input_ack().await.next_expected_frame, 1);
    assert_eq!(host.expect_strict_input_from(1).await.start_frame, 0);

    host.send_host_frame_open(0).await;
    tokio::time::sleep(std::time::Duration::from_millis(
        scheduled_delay_ms.saturating_add(100),
    ))
    .await;
    let status_after_open = server.room_status().await;
    assert_eq!(
        status_after_open["room"]["status"], "playing",
        "host frame open did not release: {status_after_open}"
    );
    let host_release = host.expect_v5_release().await;
    let guest_release = guest.expect_v5_release().await;
    assert_eq!(host_release.released_frame, 0);
    assert_eq!(guest_release, host_release);
    assert_eq!(host_release.next_host_frame, 1);
    assert_eq!(server.metrics().v5_frame_releases_total, 1);

    let (_, old_session_epoch) = host.epochs();
    host.send(json!({
        "type": "stateHash",
        "report": { "frame": 0, "sha256": "a".repeat(64) }
    }))
    .await;
    guest
        .send(json!({
            "type": "stateHash",
            "report": { "frame": 0, "sha256": "b".repeat(64) }
        }))
        .await;

    let host_prepare = host.expect_type("stateRecoveryPrepare").await;
    let guest_prepare = guest.expect_type("stateRecoveryPrepare").await;
    assert_eq!(host_prepare["sessionEpoch"], old_session_epoch);
    assert_eq!(guest_prepare["room"]["status"], "repairingState");
    let recovery_id = host_prepare["recovery"]["recoveryId"]
        .as_u64()
        .expect("recovery id");

    host.send_strict_input(1, &[3]).await;
    let transition_nack = host.expect_input_nack().await;
    assert_eq!(transition_nack.expected_frame, 1);
    assert_eq!(transition_nack.reason, InputCursorNackReason::SessionState);

    let repair_bytes = [9_u8, 8, 7, 6];
    let pinned_manifest = json!({
        "snapshotId": "snapshot-1",
        "repairFrame": 0,
        "totalBytes": repair_bytes.len(),
        "sha256": format!("{:x}", Sha256::digest(repair_bytes))
    });

    host.send(json!({
        "type": "stateRecoveryPinned",
        "pin": {
            "recoveryId": recovery_id,
            "manifest": pinned_manifest.clone()
        }
    }))
    .await;

    let host_commit = host.expect_type("stateRecoveryCommitted").await;
    let guest_commit = guest.expect_type("stateRecoveryCommitted").await;
    assert_eq!(host_commit["sessionEpoch"], old_session_epoch + 1);
    assert_eq!(guest_commit["room"]["status"], "checkingCompatibility");
    assert_eq!(host_commit["recovery"]["pinnedSnapshot"], pinned_manifest);

    move_v5_pair_to_syncing(&mut host, &mut guest, "authoritative").await;
    host.send(json!({
        "type": "snapshotChunk",
        "chunk": {
            "snapshotId": "substituted",
            "repairFrame": 0,
            "index": 0,
            "bytes": repair_bytes
        }
    }))
    .await;
    host.expect_error("snapshotInvalid").await;

    relay_snapshot(&mut host, &mut guest, &repair_bytes).await;
}

#[tokio::test]
async fn unsupported_v5_binary_message_closes_only_that_input_socket() {
    let server = SmokeServer::start().await;
    server.create_v5_room().await;
    let (mut host, _guest) = connect_v5_pair(&server).await;

    host.send_raw_input(b"BAD!".to_vec()).await;
    host.expect_input_socket_closed().await;
}

#[tokio::test]
async fn a_v4_socket_cannot_attach_to_an_exact_v5_room() {
    let server = SmokeServer::start().await;
    server.create_v5_room().await;
    let error = connect_async(format!(
        "{}/v1/ws?inviteCode=AB23-CD&role=guest&protocolVersion=4",
        server.ws_base
    ))
    .await
    .expect_err("mixed protocol join must fail");

    let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
        panic!("expected HTTP protocol rejection");
    };
    assert_eq!(response.status().as_u16(), 400);
}

async fn relay_snapshot(host: &mut SmokeClient, guest: &mut SmokeClient, bytes: &[u8]) {
    let (chunk, complete) = snapshot_payload(bytes);
    host.send(chunk).await;
    guest.expect_type("snapshotChunk").await;
    host.send(complete).await;
    guest.expect_type("snapshotComplete").await;
}

async fn schedule_start(host: &mut SmokeClient, guest: &mut SmokeClient) -> u64 {
    host.send(json!({ "type": "ready" })).await;
    guest.send(json!({ "type": "ready" })).await;
    let host_request = host.expect_type("clockSyncSampleRequested").await;
    let guest_request = guest.expect_type("clockSyncSampleRequested").await;
    assert_eq!(host_request["request"], guest_request["request"]);

    send_clock_samples(host, &host_request["request"], 100).await;
    send_clock_samples(guest, &guest_request["request"], 200).await;
    host.send(json!({
        "type": "deterministicReady",
        "report": {
            "localReadyTimeMs": 300,
            "warmupFrameCount": 30,
            "loadedStateFrame": 0
        }
    }))
    .await;
    guest
        .send(json!({
            "type": "deterministicReady",
            "report": {
                "localReadyTimeMs": 400,
                "warmupFrameCount": 30,
                "loadedStateFrame": 0
            }
        }))
        .await;

    let host_start = host.expect_type("startSession").await;
    let guest_start = guest.expect_type("startSession").await;
    assert!(host_start["scheduledStart"].is_object());
    assert!(guest_start["scheduledStart"].is_object());
    host_start["scheduledStart"]["serverTimeMs"]
        .as_u64()
        .expect("scheduled server time")
        .saturating_sub(
            host_start["scheduledStart"]["createdAtServerTimeMs"]
                .as_u64()
                .expect("scheduled creation time"),
        )
}

async fn send_clock_samples(client: &mut SmokeClient, request: &Value, base_ms: u64) {
    let count = request["requestedSampleCount"]
        .as_u64()
        .expect("sample count");
    for sample_index in 0..count {
        client
            .send(json!({
                "type": "clockSyncSample",
                "sample": {
                    "requestId": request["requestId"],
                    "sampleIndex": sample_index,
                    "serverSendTimeMs": request["serverSendTimeMs"],
                    "clientReceiveTimeMs": base_ms + sample_index * 10,
                    "clientSendTimeMs": base_ms + sample_index * 10 + 2
                }
            }))
            .await;
    }
}
