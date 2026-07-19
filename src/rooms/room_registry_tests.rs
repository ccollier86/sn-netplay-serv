//! Tests for in-memory room registry behavior.
//!
//! The registry is the synchronization boundary for active rooms, so these
//! tests cover lookup, joins, and room event publication.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    ClientRuntimeState, CompatibilityFingerprint, InputFrame, InputFrameBatch,
    LEGACY_NETPLAY_PROTOCOL_VERSION, NetplaySessionDescriptor, SessionPauseReason, SnapshotChunk,
    SnapshotManifest, StateHashReport,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex, PlayerRuntimeState, PlayerStatus,
    RoomError, RoomEvent, RoomStatus,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct StaticInviteCodeGenerator;

impl InviteCodeGenerator for StaticInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        InviteCode::parse("AB23-CD").expect("invite")
    }
}

#[tokio::test]
async fn created_room_can_be_found_by_invite_code() {
    let registry = registry();
    let view = registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");

    let found = registry
        .room_view(InviteCode::parse(view.invite_code).expect("invite"))
        .await
        .expect("found");

    assert_eq!(found.room_id, view.room_id);
}

#[tokio::test]
async fn first_guest_receives_player_two() {
    let registry = registry();
    let view = registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");

    let player_index = registry
        .join_guest(
            InviteCode::parse(view.invite_code).expect("invite"),
            license("guest"),
            ConnectionId::new(),
        )
        .await
        .expect("guest");

    assert_eq!(player_index, PlayerIndex::TWO);
}

#[tokio::test]
async fn connect_guest_returns_joined_room_view() {
    let registry = registry();
    let view = registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");

    let join = registry
        .connect_guest(
            InviteCode::parse(view.invite_code).expect("invite"),
            license("guest"),
            ConnectionId::new(),
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest");

    assert_eq!(join.player_index, PlayerIndex::TWO);
    assert!(join.room.players[1].occupied);
}

#[tokio::test]
async fn join_broadcasts_room_state_event() {
    let registry = registry();
    let view = registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .connect_guest(
            invite,
            license("guest"),
            ConnectionId::new(),
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest");

    let event = events.recv().await.expect("event");

    assert!(matches!(
        event,
        crate::rooms::RoomEvent::RoomStateChanged(_)
    ));
}

#[tokio::test]
async fn third_player_is_rejected() {
    let registry = registry();
    let view = registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");

    registry
        .join_guest(invite.clone(), license("guest"), ConnectionId::new())
        .await
        .expect("guest");
    let result = registry
        .join_guest(invite, license("third"), ConnectionId::new())
        .await;

    assert!(matches!(result, Err(RoomError::RoomFull)));
}

#[tokio::test]
async fn waiting_rooms_expire_after_join_timeout() {
    let registry = registry();
    registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");

    let expired_count = registry
        .remove_expired_waiting_rooms(
            Instant::now() + Duration::from_secs(601),
            Duration::from_secs(600),
        )
        .await;

    assert_eq!(expired_count, 1);
}

#[tokio::test]
async fn joined_rooms_do_not_expire_as_waiting_rooms() {
    let registry = registry();
    let view = registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");

    registry
        .connect_guest(
            InviteCode::parse(view.invite_code).expect("invite"),
            license("guest"),
            ConnectionId::new(),
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest");

    let expired_count = registry
        .remove_expired_waiting_rooms(
            Instant::now() + Duration::from_secs(601),
            Duration::from_secs(600),
        )
        .await;

    assert_eq!(expired_count, 0);
}

#[tokio::test]
async fn host_disconnect_removes_closed_room() {
    let registry = registry();
    let host_connection = ConnectionId::new();
    let view = registry
        .create_room(license("host"), host_connection, descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");

    registry
        .disconnect(invite.clone(), host_connection)
        .await
        .expect("disconnect");

    assert!(matches!(
        registry.room_view(invite).await,
        Err(RoomError::NotFound)
    ));
}

#[tokio::test]
async fn player_exit_broadcasts_and_closes_room() {
    let (registry, invite, _host_connection, guest_connection) = compatible_room().await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    let room = registry
        .player_exited(invite.clone(), guest_connection, "userQuit".to_string())
        .await
        .expect("player exited");
    let event = events.recv().await.expect("event");

    assert_eq!(room.status, RoomStatus::Closed);
    assert!(matches!(
        event,
        RoomEvent::PlayerExited {
            player_index: 1,
            ..
        }
    ));
}

#[tokio::test]
async fn compatibility_mismatch_broadcasts_room_state() {
    let registry = registry();
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let view = registry
        .create_room(license("host"), host_connection, descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");

    registry
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_connection,
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest");
    registry
        .set_compatibility(invite.clone(), host_connection, fingerprint("rom"))
        .await
        .expect("host fingerprint");
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    let result = registry
        .set_compatibility(invite, guest_connection, fingerprint("rom-b"))
        .await;
    let event = events.recv().await.expect("event");

    assert!(matches!(result, Err(RoomError::CompatibilityMismatch)));
    assert!(matches!(event, RoomEvent::RoomStateChanged(_)));
}

#[tokio::test]
async fn ready_from_both_players_broadcasts_session_start() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite, guest_connection, None)
        .await
        .expect("guest ready");

    let first_event = events.recv().await.expect("first event");
    let second_event = events.recv().await.expect("second event");

    assert!(matches!(
        first_event,
        crate::rooms::RoomEvent::RoomStateChanged(_)
    ));
    assert!(matches!(
        second_event,
        crate::rooms::RoomEvent::SessionStarted { .. }
    ));
}

#[tokio::test]
async fn host_snapshot_chunk_is_broadcast() {
    let (registry, invite, host_connection, _guest_connection) = compatible_room().await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .relay_snapshot_chunk(
            invite,
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

    let event = events.recv().await.expect("event");

    assert!(matches!(
        event,
        crate::rooms::RoomEvent::SnapshotChunk { .. }
    ));
}

#[tokio::test]
async fn snapshot_chunks_do_not_publish_to_input_channel() {
    let (registry, invite, host_connection, _guest_connection) = compatible_room().await;
    let mut input_events = registry
        .subscribe_input(invite.clone())
        .await
        .expect("input events");

    registry
        .relay_snapshot_chunk(
            invite,
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

    assert!(matches!(
        input_events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn snapshot_complete_requires_matching_chunks() {
    let (registry, invite, host_connection, _guest_connection) = compatible_room().await;

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

    let result = registry
        .relay_snapshot_complete(
            invite,
            host_connection,
            SnapshotManifest {
                snapshot_id: "snapshot-1".to_string(),
                repair_frame: 0,
                total_bytes: 3,
                sha256: "0".repeat(64),
            },
        )
        .await;

    assert!(matches!(result, Err(RoomError::SnapshotInvalid)));
}

#[tokio::test]
async fn host_snapshot_complete_is_broadcast_after_valid_chunks() {
    let (registry, invite, host_connection, _guest_connection) = compatible_room().await;

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
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .relay_snapshot_complete(invite, host_connection, snapshot_manifest(&[1, 2, 3]))
        .await
        .expect("snapshot complete");

    let event = events.recv().await.expect("event");

    assert!(matches!(event, RoomEvent::SnapshotComplete { .. }));
}

#[tokio::test]
async fn host_input_releases_server_frame_before_guest_input_arrives() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let mut events = registry
        .subscribe_input(invite.clone())
        .await
        .expect("input events");
    let room_epoch = room_epoch(&registry, &invite).await;
    let session_epoch = session_epoch(&registry, &invite).await;

    registry
        .relay_input_frame_batch(
            invite.clone(),
            host_connection,
            InputFrameBatch {
                room_epoch,
                session_epoch,
                player_index: PlayerIndex::ONE,
                frames: vec![InputFrame {
                    player_index: PlayerIndex::ONE,
                    frame: 0,
                    payload: vec![0],
                }],
            },
        )
        .await
        .expect("input frame");

    assert_eq!(registry.release_next_controller_frames().await, 1);

    let event = events.recv().await.expect("host input event");
    assert!(matches!(
        event,
        crate::rooms::RoomInputEvent::InputFrameBatch { .. }
    ));

    let event = events.recv().await.expect("server frame event");
    assert!(matches!(
        event,
        crate::rooms::RoomInputEvent::ServerFrame { .. }
    ));

    registry
        .relay_input_frame_batch(
            invite.clone(),
            guest_connection,
            InputFrameBatch {
                room_epoch,
                session_epoch,
                player_index: PlayerIndex::TWO,
                frames: vec![InputFrame {
                    player_index: PlayerIndex::TWO,
                    frame: 0,
                    payload: vec![0],
                }],
            },
        )
        .await
        .expect("guest input frame");

    let event = events.recv().await.expect("late guest input event");
    assert!(matches!(
        event,
        crate::rooms::RoomInputEvent::InputFrameBatch { .. }
    ));
}

#[tokio::test]
async fn server_frame_clock_follows_host_input_cursor() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let room_epoch = room_epoch(&registry, &invite).await;
    let session_epoch = session_epoch(&registry, &invite).await;

    registry
        .relay_input_frame_batch(
            invite.clone(),
            host_connection,
            InputFrameBatch {
                room_epoch,
                session_epoch,
                player_index: PlayerIndex::ONE,
                frames: vec![input(PlayerIndex::ONE, 0), input(PlayerIndex::ONE, 1)],
            },
        )
        .await
        .expect("host input");

    assert_eq!(registry.release_next_controller_frames().await, 1);
    assert_eq!(registry.release_next_controller_frames().await, 1);
    assert_eq!(registry.release_next_controller_frames().await, 0);

    let room = registry
        .room_view(invite)
        .await
        .expect("room should remain active");

    assert!(matches!(room.frame_clock.released_frame, Some(1)));
}

#[tokio::test]
async fn future_host_input_waits_for_released_frame_before_broadcast() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let mut events = registry
        .subscribe_input(invite.clone())
        .await
        .expect("input events");
    let room_epoch = room_epoch(&registry, &invite).await;
    let session_epoch = session_epoch(&registry, &invite).await;

    registry
        .relay_input_frame_batch(
            invite.clone(),
            host_connection,
            InputFrameBatch {
                room_epoch,
                session_epoch,
                player_index: PlayerIndex::ONE,
                frames: (0..=3)
                    .map(|frame| input(PlayerIndex::ONE, frame))
                    .collect(),
            },
        )
        .await
        .expect("host future input");

    registry
        .relay_input_frame_batch(
            invite.clone(),
            host_connection,
            InputFrameBatch {
                room_epoch,
                session_epoch,
                player_index: PlayerIndex::ONE,
                frames: (4..=5)
                    .map(|frame| input(PlayerIndex::ONE, frame))
                    .collect(),
            },
        )
        .await
        .expect("more host future input");

    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));

    for _ in 0..=5 {
        registry.release_next_controller_frames().await;
    }

    let mut host_input_batch_count = 0;
    while host_input_batch_count < 6 {
        let event = events.recv().await.expect("input event");
        if matches!(event, crate::rooms::RoomInputEvent::InputFrameBatch { .. }) {
            host_input_batch_count += 1;
        }
    }

    assert_eq!(host_input_batch_count, 6);
}

#[tokio::test]
async fn state_hash_mismatch_requests_snapshot_resync() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let session_epoch = session_epoch(&registry, &invite).await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .record_state_hash(invite.clone(), host_connection, state_hash(60, "a"))
        .await
        .expect("host hash");
    registry
        .record_state_hash(invite.clone(), guest_connection, state_hash(60, "b"))
        .await
        .expect("guest hash");

    let event = events.recv().await.expect("state hash event");
    let RoomEvent::StateHashMismatch { mismatch, room } = event else {
        panic!("expected state hash mismatch event");
    };

    assert_eq!(mismatch.frame, 60);
    assert_eq!(mismatch.repair_frame, 60);
    assert_eq!(room.status, RoomStatus::CheckingCompatibility);
    assert_eq!(room.session_epoch, session_epoch + 1);
    assert_eq!(room.frame_clock.canonical_frame, 60);

    let room = registry
        .room_view(invite.clone())
        .await
        .expect("room remains available for resync");
    assert_eq!(room.status, RoomStatus::CheckingCompatibility);
    assert_eq!(room.session_epoch, session_epoch + 1);

    let debug_events = registry
        .room_events(invite.clone(), 1)
        .await
        .expect("debug events");
    assert_eq!(
        debug_events.first().map(|event| event.kind.as_str()),
        Some("stateHashResyncRequired")
    );
}

#[tokio::test]
async fn matching_state_hash_is_recorded_for_telemetry() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .record_state_hash(invite.clone(), host_connection, state_hash(60, "a"))
        .await
        .expect("host hash");
    registry
        .record_state_hash(invite.clone(), guest_connection, state_hash(60, "a"))
        .await
        .expect("guest hash");

    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));

    let debug_events = registry
        .room_events(invite.clone(), 1)
        .await
        .expect("debug events");
    assert_eq!(
        debug_events.first().map(|event| event.kind.as_str()),
        Some("stateHashMatched")
    );
    assert!(
        debug_events
            .first()
            .is_some_and(|event| event.detail.contains("frame 60"))
    );
}

#[tokio::test]
async fn nearby_state_hash_match_is_diagnostic_on_resync_event() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let session_epoch = session_epoch(&registry, &invite).await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .record_state_hash(invite.clone(), guest_connection, state_hash(55, "a"))
        .await
        .expect("guest nearby hash");
    registry
        .record_state_hash(invite.clone(), host_connection, state_hash(60, "a"))
        .await
        .expect("host hash");
    registry
        .record_state_hash(invite.clone(), guest_connection, state_hash(60, "b"))
        .await
        .expect("guest hash");

    let event = events.recv().await.expect("state hash event");
    let RoomEvent::StateHashMismatch { mismatch, room } = event else {
        panic!("expected state hash mismatch event");
    };

    assert_eq!(mismatch.frame, 60);
    assert_eq!(mismatch.repair_frame, 60);
    assert_eq!(mismatch.nearby_matches.len(), 1);
    assert_eq!(room.status, RoomStatus::CheckingCompatibility);
    assert_eq!(room.session_epoch, session_epoch + 1);

    let room = registry
        .room_view(invite.clone())
        .await
        .expect("room remains available for resync");
    assert_eq!(room.status, RoomStatus::CheckingCompatibility);
    assert_eq!(room.session_epoch, session_epoch + 1);

    let debug_events = registry
        .room_events(invite.clone(), 1)
        .await
        .expect("debug events");
    assert_eq!(
        debug_events.first().map(|event| event.kind.as_str()),
        Some("stateHashResyncRequired")
    );
    assert!(
        debug_events
            .first()
            .is_some_and(|event| event.detail.contains("nearby-frame match"))
    );
}

#[tokio::test]
async fn dynamic_state_hash_window_uses_reported_local_frame_spread() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let session_epoch = session_epoch(&registry, &invite).await;

    registry
        .record_heartbeat(
            invite.clone(),
            host_connection,
            0,
            Some(60),
            ClientRuntimeState::Playing,
            None,
        )
        .await
        .expect("host heartbeat");
    registry
        .record_heartbeat(
            invite.clone(),
            guest_connection,
            0,
            Some(75),
            ClientRuntimeState::Playing,
            None,
        )
        .await
        .expect("guest heartbeat");

    registry
        .record_state_hash(invite.clone(), guest_connection, state_hash(75, "a"))
        .await
        .expect("guest nearby hash");
    registry
        .record_state_hash(invite.clone(), host_connection, state_hash(60, "a"))
        .await
        .expect("host hash");
    registry
        .record_state_hash(invite.clone(), guest_connection, state_hash(60, "b"))
        .await
        .expect("guest hash");

    let room = registry
        .room_view(invite.clone())
        .await
        .expect("room remains available for resync");
    assert_eq!(room.status, RoomStatus::CheckingCompatibility);
    assert_eq!(room.session_epoch, session_epoch + 1);

    let debug_events = registry
        .room_events(invite.clone(), 1)
        .await
        .expect("debug events");
    assert_eq!(
        debug_events.first().map(|event| event.kind.as_str()),
        Some("stateHashResyncRequired")
    );
    assert!(
        debug_events
            .first()
            .is_some_and(|event| event.detail.contains("first offset 15"))
    );
}

#[tokio::test]
async fn first_confirmed_state_hash_mismatch_requires_snapshot_resync() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let session_epoch = session_epoch(&registry, &invite).await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .record_state_hash(invite.clone(), host_connection, state_hash(180, "e"))
        .await
        .expect("host hash");
    registry
        .record_state_hash(invite.clone(), guest_connection, state_hash(180, "f"))
        .await
        .expect("guest hash");

    let event = events.recv().await.expect("state hash event");
    let RoomEvent::StateHashMismatch { mismatch, room } = event else {
        panic!("expected state hash mismatch event");
    };

    assert_eq!(mismatch.frame, 180);
    assert_eq!(mismatch.repair_frame, 180);
    assert_eq!(room.status, RoomStatus::CheckingCompatibility);
    assert_eq!(room.session_epoch, session_epoch + 1);
    assert_eq!(room.frame_clock.canonical_frame, 180);
    assert_eq!(room.players[0].status, PlayerStatus::Connected);
    assert_eq!(room.players[1].status, PlayerStatus::Connected);

    let debug_events = registry
        .room_events(invite.clone(), 1)
        .await
        .expect("debug events");
    assert_eq!(
        debug_events.first().map(|event| event.kind.as_str()),
        Some("stateHashResyncRequired")
    );
}

#[tokio::test]
async fn repeated_pause_request_updates_existing_pause() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .request_session_pause(
            invite.clone(),
            host_connection,
            SessionPauseReason::Menu,
            10,
        )
        .await
        .expect("host pause");
    assert!(matches!(
        events.recv().await.expect("scheduled event"),
        RoomEvent::SessionPauseScheduled { .. }
    ));

    registry
        .request_session_pause(invite, guest_connection, SessionPauseReason::Menu, 10)
        .await
        .expect("guest pause");
    assert!(matches!(
        events.recv().await.expect("updated event"),
        RoomEvent::SessionPauseUpdated { .. }
    ));
}

#[tokio::test]
async fn transport_close_records_actor_detail_without_broadcasting_state() {
    let (registry, invite, _host_connection, guest_connection) = compatible_room().await;
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .record_transport_close(
            invite.clone(),
            guest_connection,
            "control",
            "peer close frame code=1000 reason=android runtime failed".to_string(),
        )
        .await
        .expect("transport close diagnostic");

    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));

    let debug_events = registry.room_events(invite, 1).await.expect("debug events");
    let event = debug_events.first().expect("event");
    assert_eq!(event.kind, "socketTransportClosed");
    assert!(event.detail.contains("p2"));
    assert!(event.detail.contains("role=guest"));
    assert!(event.detail.contains("client=desktop"));
    assert!(event.detail.contains("reason=peer close frame code=1000"));
}

#[tokio::test]
async fn reconnect_with_valid_resume_token_restores_player_slot() {
    let (registry, invite, _host_connection, guest_connection, _host_token, guest_token) =
        reconnectable_room().await;
    let known_epoch = registry
        .room_view(invite.clone())
        .await
        .expect("room before disconnect")
        .room_epoch;

    registry
        .disconnect(invite.clone(), guest_connection)
        .await
        .expect("guest disconnect");
    let reconnect = registry
        .reconnect_player(
            invite,
            PlayerIndex::TWO,
            known_epoch,
            guest_token,
            ConnectionId::new(),
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest reconnect");

    assert_eq!(reconnect.player_index, PlayerIndex::TWO);
    assert_eq!(reconnect.room.status, RoomStatus::CheckingCompatibility);
    assert_eq!(reconnect.room.players[1].status, PlayerStatus::Connected);
    assert_eq!(
        reconnect.room.players[1].runtime_state,
        PlayerRuntimeState::Connected
    );
}

#[tokio::test]
async fn reconnect_rejects_stale_room_epoch() {
    let (registry, invite, _host_connection, guest_connection, _host_token, guest_token) =
        reconnectable_room().await;
    let known_epoch = registry
        .room_view(invite.clone())
        .await
        .expect("room before disconnect")
        .room_epoch;

    registry
        .disconnect(invite.clone(), guest_connection)
        .await
        .expect("guest disconnect");
    let stale_epoch = known_epoch.saturating_sub(1);
    let result = registry
        .reconnect_player(
            invite,
            PlayerIndex::TWO,
            stale_epoch,
            guest_token,
            ConnectionId::new(),
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await;

    assert!(matches!(result, Err(RoomError::StaleRoomEpoch)));
}

#[tokio::test]
async fn heartbeat_timeout_enters_recovery_then_expires_room() {
    let (registry, invite, _host_connection, _guest_connection, _host_token, _guest_token) =
        reconnectable_room().await;
    let stale_at = Instant::now() + Duration::from_secs(31);

    let first_removed = registry
        .remove_expired_waiting_rooms(stale_at, Duration::from_secs(600))
        .await;
    let recovering = registry
        .room_view(invite.clone())
        .await
        .expect("recovering room");

    assert_eq!(first_removed, 0);
    assert_eq!(recovering.status, RoomStatus::Recovering);
    assert_eq!(recovering.players[0].status, PlayerStatus::Reconnecting);
    assert_eq!(recovering.players[1].status, PlayerStatus::Reconnecting);

    let second_removed = registry
        .remove_expired_waiting_rooms(stale_at + Duration::from_secs(91), Duration::from_secs(600))
        .await;

    assert_eq!(second_removed, 1);
    assert!(matches!(
        registry.room_view(invite).await,
        Err(RoomError::NotFound)
    ));
}

#[tokio::test]
async fn heartbeat_stale_updates_runtime_before_recovery_timeout() {
    let (registry, invite, _host_connection, _guest_connection, _host_token, _guest_token) =
        reconnectable_room().await;

    let removed = registry
        .remove_expired_waiting_rooms(
            Instant::now() + Duration::from_secs(16),
            Duration::from_secs(600),
        )
        .await;
    let room = registry.room_view(invite).await.expect("room");

    assert_eq!(removed, 0);
    assert_eq!(room.status, RoomStatus::Playing);
    assert_eq!(room.players[0].runtime_state, PlayerRuntimeState::Stale);
    assert_eq!(room.players[1].runtime_state, PlayerRuntimeState::Stale);
}

#[tokio::test]
async fn second_disconnect_during_recovery_keeps_both_slots_reconnectable() {
    let (registry, invite, host_connection, guest_connection, host_token, guest_token) =
        reconnectable_room().await;
    let known_epoch = registry
        .room_view(invite.clone())
        .await
        .expect("room before disconnect")
        .room_epoch;

    registry
        .disconnect(invite.clone(), guest_connection)
        .await
        .expect("guest disconnect");
    let recovering = registry
        .disconnect(invite.clone(), host_connection)
        .await
        .expect("host disconnect during recovery");

    assert_eq!(recovering.status, RoomStatus::Recovering);
    assert_eq!(recovering.players[0].status, PlayerStatus::Reconnecting);
    assert_eq!(recovering.players[1].status, PlayerStatus::Reconnecting);

    let host_reconnect = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::ONE,
            known_epoch,
            host_token,
            ConnectionId::new(),
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("host reconnect");
    let guest_reconnect = registry
        .reconnect_player(
            invite,
            PlayerIndex::TWO,
            known_epoch,
            guest_token,
            ConnectionId::new(),
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest reconnect");

    assert_eq!(
        host_reconnect.room.players[0].status,
        PlayerStatus::Connected
    );
    assert_eq!(
        guest_reconnect.room.players[1].status,
        PlayerStatus::Connected
    );
}

fn registry() -> InMemoryRoomRegistry {
    InMemoryRoomRegistry::new(Arc::new(StaticInviteCodeGenerator))
}

async fn compatible_room() -> (InMemoryRoomRegistry, InviteCode, ConnectionId, ConnectionId) {
    let registry = registry();
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
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("host");
    let guest_join = registry
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_connection,
            crate::rooms::ClientTransportCapabilities::default(),
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
            PlayerIndex::ONE,
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
            PlayerIndex::TWO,
            view.room_epoch,
            view.session_epoch,
            guest_token.to_string(),
            guest_connection,
        )
        .await
        .expect("guest input socket");
}

async fn reconnectable_room() -> (
    InMemoryRoomRegistry,
    InviteCode,
    ConnectionId,
    ConnectionId,
    String,
    String,
) {
    let registry = registry();
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
            crate::rooms::ClientTransportCapabilities::default(),
        )
        .await
        .expect("host");
    let guest_join = registry
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_connection,
            crate::rooms::ClientTransportCapabilities::default(),
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
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");

    (
        registry,
        invite,
        host_connection,
        guest_connection,
        host_join.resume_token,
        guest_join.resume_token,
    )
}

async fn room_epoch(registry: &InMemoryRoomRegistry, invite: &InviteCode) -> u64 {
    registry
        .room_view(invite.clone())
        .await
        .expect("room")
        .room_epoch
}

async fn session_epoch(registry: &InMemoryRoomRegistry, invite: &InviteCode) -> u64 {
    registry
        .room_view(invite.clone())
        .await
        .expect("room")
        .session_epoch
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
        protocol_version: LEGACY_NETPLAY_PROTOCOL_VERSION,
        system_id: "gamecube".to_string(),
        core_id: "dolphin".to_string(),
        core_build: "core-build".to_string(),
        state_format: Some("dolphin:gamecube:libretro-serialize-v1".to_string()),
        content_hash: content_hash_for_fixture(content_hash),
        settings_hash: "settings".to_string(),
        cheats_hash: "cheats".to_string(),
        system_data_hash: None,
        save_data_mode: "netplay".to_string(),
        determinism_v5: None,
    }
}

fn input(player_index: PlayerIndex, frame: u64) -> InputFrame {
    InputFrame {
        frame,
        payload: vec![0],
        player_index,
    }
}

fn state_hash(frame: u64, fill: &str) -> StateHashReport {
    StateHashReport {
        frame,
        sha256: fill.repeat(64),
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
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}
