mod support;

use sb_netplay_serv::protocol::InputCursorNackReason;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::{
    SmokeClient, SmokeServer, connect_v5_pair, move_v5_pair_to_syncing, snapshot_payload_at,
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
async fn delayed_large_snapshot_finishes_before_clock_generation_starts() {
    let server = SmokeServer::start().await;
    server.create_v5_room().await;
    let (mut host, mut guest) = connect_v5_pair(&server).await;
    move_v5_pair_to_syncing(&mut host, &mut guest, "diagnostic").await;

    host.send(json!({ "type": "ready" })).await;
    host.expect_error("roomNotReady").await;
    guest.send(json!({ "type": "ready" })).await;
    guest.expect_error("roomNotReady").await;

    let bytes = vec![7_u8; 128 * 1024];
    let (chunk, complete) = snapshot_payload_at(&bytes, 0);
    host.send(chunk).await;
    guest.expect_type("snapshotChunk").await;
    tokio::time::sleep(std::time::Duration::from_millis(75)).await;
    assert_eq!(server.room_status().await["room"]["status"], "syncingState");

    host.send(complete).await;
    guest.expect_type("snapshotComplete").await;
    assert!(schedule_start(&mut host, &mut guest).await > 0);
}

#[tokio::test]
async fn scheduled_host_release_and_two_phase_recovery_work_over_real_sockets() {
    let server = SmokeServer::start().await;
    server.create_v5_room().await;
    let (mut host, mut guest) = connect_v5_pair(&server).await;
    move_v5_pair_to_syncing(&mut host, &mut guest, "authoritative").await;
    relay_snapshot(&mut host, &mut guest, &[1, 2, 3, 4], 0).await;
    let scheduled_delay_ms = schedule_start(&mut host, &mut guest).await;

    host.send_strict_input(0, &[1]).await;
    assert_eq!(host.expect_input_ack().await.next_expected_frame, 1);
    assert_eq!(guest.expect_strict_input_from(0).await.start_frame, 0);
    guest.send_strict_input(0, &[2]).await;
    assert_eq!(guest.expect_input_ack().await.next_expected_frame, 1);
    assert_eq!(host.expect_strict_input_from(1).await.start_frame, 0);

    host.send_host_frame_open(0).await;
    // A queued frame-one open must be ignored while frame zero owns the
    // scheduled transition barrier; it must not close the input lane.
    host.send_host_frame_open(1).await;
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

    let (old_room_epoch, old_session_epoch) = host.epochs();
    let repair_frame = 600_u64;
    host.send(json!({
        "type": "stateHash",
        "report": { "frame": repair_frame, "sha256": "a".repeat(64) }
    }))
    .await;
    guest
        .send(json!({
            "type": "stateHash",
            "report": { "frame": repair_frame, "sha256": "b".repeat(64) }
        }))
        .await;

    let host_prepare = host.expect_type("stateRecoveryPrepare").await;
    let guest_prepare = guest.expect_type("stateRecoveryPrepare").await;
    assert_eq!(host_prepare["sessionEpoch"], old_session_epoch);
    assert_eq!(guest_prepare["room"]["status"], "repairingState");
    let recovery_id = host_prepare["recovery"]["recoveryId"]
        .as_u64()
        .expect("recovery id");

    // A host open can already be queued when the control lane freezes the old epoch.
    // It is obsolete transition work, not a reason to close the binary input socket.
    host.send_host_frame_open(1).await;
    host.send_strict_input(1, &[3]).await;
    host.expect_no_input_message().await;

    let repair_bytes = [9_u8, 8, 7, 6];
    let pinned_manifest = json!({
        "snapshotId": "snapshot-1",
        "repairFrame": repair_frame,
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

    // The control commit can overtake old-epoch input already queued on the
    // binary lane. The relay must drop it instead of stamping the new epoch on
    // a SessionState NACK that causes the client to close its input socket.
    host.send_strict_input_at_epochs(old_room_epoch, old_session_epoch, 1, &[3])
        .await;
    host.send_host_frame_open_at_epochs(old_room_epoch, old_session_epoch, 1)
        .await;
    host.expect_no_input_message().await;

    host.send(json!({
        "type": "stateRecoveryPinned",
        "roomEpoch": host_commit["roomEpoch"],
        "sessionEpoch": old_session_epoch,
        "pin": {
            "recoveryId": recovery_id,
            "manifest": pinned_manifest.clone()
        }
    }))
    .await;
    assert_eq!(
        host.expect_type("stateRecoveryCommitted").await["recovery"]["recoveryId"],
        recovery_id
    );
    assert_eq!(
        guest.expect_type("stateRecoveryCommitted").await["recovery"]["recoveryId"],
        recovery_id
    );

    move_v5_pair_to_syncing(&mut host, &mut guest, "authoritative").await;
    host.send(json!({
        "type": "snapshotChunk",
        "chunk": {
            "snapshotId": "substituted",
            "repairFrame": repair_frame,
            "index": 0,
            "bytes": repair_bytes
        }
    }))
    .await;
    host.expect_error("snapshotInvalid").await;

    relay_snapshot(&mut host, &mut guest, &repair_bytes, repair_frame).await;
    let recovery_start_delay_ms = schedule_start(&mut host, &mut guest).await;

    // A client at the recovery checkpoint must not lose the fresh epoch if a
    // digest reaches the control lane just before the host frame-open.
    host.send(json!({
        "type": "stateHash",
        "report": { "frame": repair_frame, "sha256": "c".repeat(64) }
    }))
    .await;
    guest
        .send(json!({
            "type": "stateHash",
            "report": { "frame": repair_frame, "sha256": "c".repeat(64) }
        }))
        .await;

    host.send_strict_input(repair_frame, &[4]).await;
    assert_eq!(
        host.expect_input_ack().await.next_expected_frame,
        repair_frame + 1
    );
    assert_eq!(
        guest.expect_strict_input_from(0).await.start_frame,
        repair_frame
    );
    guest.send_strict_input(repair_frame, &[5]).await;
    assert_eq!(
        guest.expect_input_ack().await.next_expected_frame,
        repair_frame + 1
    );
    assert_eq!(
        host.expect_strict_input_from(1).await.start_frame,
        repair_frame
    );

    host.send_host_frame_open(repair_frame).await;
    tokio::time::sleep(std::time::Duration::from_millis(
        recovery_start_delay_ms.saturating_add(100),
    ))
    .await;
    assert_eq!(host.expect_v5_release().await.released_frame, repair_frame);
    assert_eq!(guest.expect_v5_release().await.released_frame, repair_frame);

    let recovered_status = server.room_status().await;
    assert_eq!(recovered_status["room"]["status"], "playing");
    assert_eq!(server.metrics().v5_frame_releases_total, 2);
}

#[tokio::test]
async fn pause_control_ack_cannot_overtake_input_and_resume_starts_at_p_plus_one() {
    let server = SmokeServer::start().await;
    server.create_v5_room().await;
    let (mut host, mut guest) = connect_v5_pair(&server).await;
    move_v5_pair_to_syncing(&mut host, &mut guest, "diagnostic").await;
    relay_snapshot(&mut host, &mut guest, &[1, 2, 3, 4], 0).await;
    let scheduled_delay_ms = schedule_start(&mut host, &mut guest).await;

    host.send_strict_input(0, &[1]).await;
    host.expect_input_ack().await;
    guest.expect_strict_input_from(0).await;
    guest.send_strict_input(0, &[2]).await;
    guest.expect_input_ack().await;
    host.expect_strict_input_from(1).await;
    host.send_host_frame_open(0).await;
    tokio::time::sleep(std::time::Duration::from_millis(
        scheduled_delay_ms.saturating_add(100),
    ))
    .await;
    host.expect_v5_release().await;
    guest.expect_v5_release().await;

    host.send(json!({
        "type": "requestSessionPause",
        "requestId": "pause-race",
        "reason": "menu",
        "localFrame": 0
    }))
    .await;
    let host_pause = host.expect_type("sessionPauseScheduled").await;
    let guest_pause = guest.expect_type("sessionPauseScheduled").await;
    let sequence = host_pause["pause"]["sequence"].as_u64().expect("sequence");
    let pause_at_frame = host_pause["pause"]["pauseAtFrame"]
        .as_u64()
        .expect("pause frame");
    assert_eq!(guest_pause["pause"]["pauseAtFrame"], pause_at_frame);

    // The control lane wins this race, but the relay must not acknowledge it
    // until the binary lane proves release and both accepted cursors through P.
    host.send(json!({
        "type": "sessionPauseReached",
        "sequence": sequence,
        "pausedAtFrame": pause_at_frame
    }))
    .await;
    host.expect_error("roomNotReady").await;
    assert_eq!(
        server.room_status().await["room"]["pause"]["acknowledgedPlayerIndexes"],
        json!([])
    );

    for start_frame in [1_u64, 5] {
        host.send_strict_input(start_frame, &[3, 3, 3, 3]).await;
        host.expect_input_ack().await;
        guest.expect_strict_input_from(0).await;
        guest.send_strict_input(start_frame, &[4, 4, 4, 4]).await;
        guest.expect_input_ack().await;
        host.expect_strict_input_from(1).await;
    }
    for frame in 1..=pause_at_frame {
        host.send_host_frame_open(frame).await;
    }
    for frame in 1..=pause_at_frame {
        assert_eq!(host.expect_v5_release().await.released_frame, frame);
        assert_eq!(guest.expect_v5_release().await.released_frame, frame);
    }

    host.send(json!({
        "type": "sessionPauseReached",
        "sequence": sequence,
        "pausedAtFrame": pause_at_frame
    }))
    .await;
    host.expect_type("sessionPauseUpdated").await;
    guest.expect_type("sessionPauseUpdated").await;
    guest
        .send(json!({
            "type": "sessionPauseReached",
            "sequence": sequence,
            "pausedAtFrame": pause_at_frame
        }))
        .await;
    let host_paused = host.expect_type("sessionPauseUpdated").await;
    let guest_paused = guest.expect_type("sessionPauseUpdated").await;
    assert_eq!(host_paused["pause"]["state"], "paused");
    assert_eq!(guest_paused["pause"]["state"], "paused");
    let (old_room_epoch, old_session_epoch) = host.epochs();

    host.send(json!({
        "type": "requestSessionResume",
        "requestId": "resume-race",
        "reason": "menu",
        "sequence": sequence
    }))
    .await;
    let host_resume = host.expect_type("sessionResumeScheduled").await;
    let guest_resume = guest.expect_type("sessionResumeScheduled").await;
    assert_eq!(host_resume["resumeAtFrame"], pause_at_frame + 1);
    assert_eq!(guest_resume["resumeAtFrame"], pause_at_frame + 1);
    assert_eq!(
        host_resume["scheduledStart"]["startFrame"],
        pause_at_frame + 1
    );
    let resume_delay_ms = host_resume["scheduledStart"]["serverTimeMs"]
        .as_u64()
        .expect("resume server time")
        .saturating_sub(
            host_resume["scheduledStart"]["createdAtServerTimeMs"]
                .as_u64()
                .expect("resume creation time"),
        );

    // Old-epoch transition work is dropped without a misleading current-epoch
    // NACK, and the same input sockets remain usable for P+1.
    host.send_strict_input_at_epochs(old_room_epoch, old_session_epoch, pause_at_frame + 1, &[5])
        .await;
    host.send_host_frame_open_at_epochs(old_room_epoch, old_session_epoch, pause_at_frame + 1)
        .await;
    host.expect_no_input_message().await;

    host.send_strict_input(pause_at_frame + 1, &[5]).await;
    assert_eq!(
        host.expect_input_ack().await.next_expected_frame,
        pause_at_frame + 2
    );
    assert_eq!(
        guest.expect_strict_input_from(0).await.start_frame,
        pause_at_frame + 1
    );
    guest.send_strict_input(pause_at_frame + 1, &[6]).await;
    assert_eq!(
        guest.expect_input_ack().await.next_expected_frame,
        pause_at_frame + 2
    );
    assert_eq!(
        host.expect_strict_input_from(1).await.start_frame,
        pause_at_frame + 1
    );
    host.send_host_frame_open(pause_at_frame + 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(
        resume_delay_ms.saturating_add(100),
    ))
    .await;
    assert_eq!(
        host.expect_v5_release().await.released_frame,
        pause_at_frame + 1
    );
    assert_eq!(
        guest.expect_v5_release().await.released_frame,
        pause_at_frame + 1
    );
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

async fn relay_snapshot(
    host: &mut SmokeClient,
    guest: &mut SmokeClient,
    bytes: &[u8],
    repair_frame: u64,
) {
    let (chunk, complete) = snapshot_payload_at(bytes, repair_frame);
    host.send(chunk.clone()).await;
    guest.expect_type("snapshotChunk").await;
    host.send(chunk).await;
    host.send(complete.clone()).await;
    guest.expect_type("snapshotComplete").await;
    host.send(complete).await;
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
        "network": { "roundTripMs": 40, "jitterMs": 0 },
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
            "network": { "roundTripMs": 40, "jitterMs": 0 },
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
    assert_eq!(
        host_start["room"]["session"]["controller"]["inputDelayFrames"],
        4
    );
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
