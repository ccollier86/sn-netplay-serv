//! V2 scheduled-start tests for the room registry.
//!
//! These tests cover the control-plane startup path only. They keep scheduled
//! start assertions out of legacy registry behavior tests.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    ClockSyncSample, ClockSyncSampleRequest, CompatibilityFingerprint, DeterministicReadyReport,
    NETPLAY_PROTOCOL_VERSION, NetplaySessionDescriptor, SnapshotChunk, SnapshotManifest,
};
use crate::rooms::{ConnectionId, InviteCode, InviteCodeGenerator, RoomEvent, RoomStatus};
use sha2::{Digest, Sha256};
use std::sync::Arc;

struct StaticInviteCodeGenerator;

impl InviteCodeGenerator for StaticInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        InviteCode::parse("AB23-CD").expect("invite")
    }
}

#[tokio::test]
async fn v2_ready_requests_clock_samples_before_session_start() {
    let (registry, invite, host_connection, guest_connection) = scheduled_start_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");

    let first_event = events.recv().await.expect("first event");
    let second_event = events.recv().await.expect("second event");
    let room = registry.room_view(invite).await.expect("room");

    assert!(matches!(first_event, RoomEvent::RoomStateChanged(_)));
    assert!(matches!(
        second_event,
        RoomEvent::ClockSyncSampleRequested { .. }
    ));
    assert_eq!(room.status, RoomStatus::Ready);
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn clock_samples_and_deterministic_ready_schedule_future_start() {
    let (registry, invite, host_connection, guest_connection) = scheduled_start_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");

    let request = receive_clock_request(&mut events).await;
    send_clock_samples(&registry, &invite, host_connection, &request, 100).await;
    send_clock_samples(&registry, &invite, guest_connection, &request, 200).await;
    registry
        .mark_deterministic_ready(
            invite.clone(),
            host_connection,
            deterministic_ready_report(300),
            None,
        )
        .await
        .expect("host deterministic ready");
    registry
        .mark_deterministic_ready(
            invite.clone(),
            guest_connection,
            deterministic_ready_report(400),
            None,
        )
        .await
        .expect("guest deterministic ready");

    let scheduled_start = receive_scheduled_start(&mut events).await;
    let room = registry.room_view(invite).await.expect("room");

    assert_eq!(room.status, RoomStatus::StartScheduled);
    assert_eq!(scheduled_start.start_frame, 0);
    assert!(
        scheduled_start.server_time_ms
            >= scheduled_start
                .created_at_server_time_ms
                .saturating_add(scheduled_start.minimum_start_delay_ms)
    );
}

async fn scheduled_start_room() -> (InMemoryRoomRegistry, InviteCode, ConnectionId, ConnectionId) {
    let registry = InMemoryRoomRegistry::new(Arc::new(StaticInviteCodeGenerator));
    let create_connection = ConnectionId::new();
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let view = registry
        .create_room(license("host"), create_connection, descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");

    let host_join = registry
        .connect_host(
            invite.clone(),
            license("host"),
            host_connection,
            scheduled_start_capabilities(),
        )
        .await
        .expect("host");
    let guest_join = registry
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_connection,
            scheduled_start_capabilities(),
        )
        .await
        .expect("guest");

    registry
        .set_compatibility(invite.clone(), host_connection, fingerprint("rom"))
        .await
        .expect("host fingerprint");
    registry
        .set_compatibility(invite.clone(), guest_connection, fingerprint("rom"))
        .await
        .expect("guest fingerprint");
    connect_input_sockets(
        &registry,
        &invite,
        host_connection,
        &host_join.input_socket_token,
        guest_connection,
        &guest_join.input_socket_token,
    )
    .await;

    (registry, invite, host_connection, guest_connection)
}

fn scheduled_start_capabilities() -> crate::rooms::ClientTransportCapabilities {
    crate::rooms::ClientTransportCapabilities {
        supports_scheduled_start: true,
        supports_clock_sync: true,
        supports_fast_input_relay: true,
        ..crate::rooms::ClientTransportCapabilities::default()
    }
}

async fn receive_clock_request(
    events: &mut crate::rooms::RoomEventReceiver,
) -> ClockSyncSampleRequest {
    for _ in 0..4 {
        if let RoomEvent::ClockSyncSampleRequested { request, .. } =
            events.recv().await.expect("room event")
        {
            return request;
        }
    }

    panic!("clock-sample request was not emitted");
}

async fn send_clock_samples(
    registry: &InMemoryRoomRegistry,
    invite: &InviteCode,
    connection_id: ConnectionId,
    request: &ClockSyncSampleRequest,
    client_time_base_ms: u64,
) {
    for sample_index in 0..request.requested_sample_count {
        registry
            .record_clock_sync_sample(
                invite.clone(),
                connection_id,
                clock_sample(request, sample_index, client_time_base_ms),
            )
            .await
            .expect("clock sample accepted");
    }
}

async fn receive_scheduled_start(
    events: &mut crate::rooms::RoomEventReceiver,
) -> crate::protocol::ScheduledSessionStart {
    for _ in 0..12 {
        if let RoomEvent::SessionStarted {
            scheduled_start: Some(start),
            ..
        } = events.recv().await.expect("room event")
        {
            return start;
        }
    }

    panic!("scheduled start was not emitted");
}

fn clock_sample(
    request: &ClockSyncSampleRequest,
    sample_index: u8,
    client_time_base_ms: u64,
) -> ClockSyncSample {
    let client_receive_time_ms = client_time_base_ms + u64::from(sample_index) * 10;
    ClockSyncSample {
        request_id: request.request_id.clone(),
        sample_index,
        server_send_time_ms: request.server_send_time_ms,
        client_receive_time_ms,
        client_send_time_ms: client_receive_time_ms + 2,
    }
}

fn deterministic_ready_report(local_ready_time_ms: u64) -> DeterministicReadyReport {
    DeterministicReadyReport {
        local_ready_time_ms,
        warmup_frame_count: 30,
        loaded_state_frame: None,
        clock: None,
    }
}

async fn connect_input_sockets(
    registry: &InMemoryRoomRegistry,
    invite: &InviteCode,
    host_connection: ConnectionId,
    host_token: &str,
    guest_connection: ConnectionId,
    guest_token: &str,
) {
    let view = registry
        .room_view(invite.clone())
        .await
        .expect("room before input sockets");

    registry
        .connect_input_socket(
            invite.clone(),
            crate::rooms::PlayerIndex::ONE,
            view.room_epoch,
            view.session_epoch,
            host_token.to_string(),
            host_connection,
        )
        .await
        .expect("host input socket");
    registry
        .connect_input_socket(
            invite.clone(),
            crate::rooms::PlayerIndex::TWO,
            view.room_epoch,
            view.session_epoch,
            guest_token.to_string(),
            guest_connection,
        )
        .await
        .expect("guest input socket");
}

async fn complete_snapshot(
    registry: &InMemoryRoomRegistry,
    invite: &InviteCode,
    host_connection: ConnectionId,
) {
    registry
        .relay_snapshot_chunk(
            invite.clone(),
            host_connection,
            SnapshotChunk {
                snapshot_id: "snapshot-1".to_string(),
                repair_frame: 0,
                index: 0,
                bytes: vec![1, 2, 3],
            },
        )
        .await
        .expect("snapshot chunk");
    registry
        .relay_snapshot_complete(
            invite.clone(),
            host_connection,
            snapshot_manifest(&[1, 2, 3]),
        )
        .await
        .expect("snapshot complete");
}

fn license(subject_id: &str) -> VerifiedLicense {
    VerifiedLicense::new(subject_id, "premium", vec!["netplay".to_string()])
}

fn descriptor() -> NetplaySessionDescriptor {
    serde_json::from_value(serde_json::json!({
        "hostAppVersion": "0.3.0",
        "game": {
            "systemId": "gamecube",
            "title": "Star Fox Adventures",
            "romSha256": "a".repeat(64),
            "contentKey": "gamecube-star-fox-adventures-usa"
        },
        "core": {
            "coreId": "dolphin",
            "stateFormat": "dolphin:gamecube:libretro-serialize-v1"
        },
        "controller": {
            "inputDelayFrames": 3
        }
    }))
    .expect("descriptor")
}

fn fingerprint(content_hash: &str) -> CompatibilityFingerprint {
    CompatibilityFingerprint {
        desktop_version: "0.2.10".to_string(),
        protocol_version: NETPLAY_PROTOCOL_VERSION,
        system_id: "gamecube".to_string(),
        core_id: "dolphin".to_string(),
        core_build: "core-build".to_string(),
        state_format: Some("dolphin:gamecube:libretro-serialize-v1".to_string()),
        content_hash: content_hash_for_fixture(content_hash),
        settings_hash: "settings".to_string(),
        cheats_hash: "cheats".to_string(),
        system_data_hash: None,
        save_data_mode: "netplay".to_string(),
    }
}

fn content_hash_for_fixture(name: &str) -> String {
    match name {
        "rom" | "rom-a" => "a".repeat(64),
        "rom-b" => "b".repeat(64),
        value => value.to_string(),
    }
}

fn snapshot_manifest(bytes: &[u8]) -> SnapshotManifest {
    SnapshotManifest {
        snapshot_id: "snapshot-1".to_string(),
        repair_frame: 0,
        total_bytes: bytes.len() as u64,
        sha256: hex_digest(bytes),
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
