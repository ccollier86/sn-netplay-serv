//! Tests for in-memory room registry behavior.
//!
//! The registry is the synchronization boundary for active rooms, so these
//! tests cover lookup, joins, and room event publication.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    CompatibilityFingerprint, InputFrame, NETPLAY_PROTOCOL_VERSION, NetplaySessionDescriptor,
    SnapshotChunk, SnapshotManifest,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex, RoomError, RoomEvent,
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
        .connect_guest(invite, license("guest"), ConnectionId::new())
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
        .connect_guest(invite.clone(), license("guest"), guest_connection)
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
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .mark_ready(invite.clone(), host_connection)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite, guest_connection)
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
async fn snapshot_complete_requires_matching_chunks() {
    let (registry, invite, host_connection, _guest_connection) = compatible_room().await;

    registry
        .relay_snapshot_chunk(
            invite.clone(),
            host_connection,
            SnapshotChunk {
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
async fn validated_input_frame_is_broadcast() {
    let (registry, invite, host_connection, guest_connection) = compatible_room().await;
    registry
        .mark_ready(invite.clone(), host_connection)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection)
        .await
        .expect("guest ready");
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    registry
        .relay_input_frame(
            invite,
            host_connection,
            InputFrame {
                player_index: PlayerIndex::ONE,
                frame: 0,
                payload: vec![0],
            },
        )
        .await
        .expect("input frame");

    let event = events.recv().await.expect("event");

    assert!(matches!(event, crate::rooms::RoomEvent::InputFrame { .. }));
}

fn registry() -> InMemoryRoomRegistry {
    InMemoryRoomRegistry::new(Arc::new(StaticInviteCodeGenerator))
}

async fn compatible_room() -> (InMemoryRoomRegistry, InviteCode, ConnectionId, ConnectionId) {
    let registry = registry();
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let view = registry
        .create_room(license("host"), host_connection, descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");

    registry
        .connect_guest(invite.clone(), license("guest"), guest_connection)
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

    (registry, invite, host_connection, guest_connection)
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
            "coreId": "dolphin"
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
        total_bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}
