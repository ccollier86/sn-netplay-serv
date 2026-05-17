//! Tests for room domain behavior.
//!
//! Keeping these in a sibling module keeps the production room model readable
//! while still testing the security-sensitive slot and frame rules directly.

use super::{NetplayRoom, RoomStatus};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    CompatibilityFingerprint, InputFrame, InputFrameLimits, SnapshotChunk, SnapshotLimits,
    SnapshotManifest,
};
use crate::rooms::{ConnectionId, InviteCode, PlayerIndex, PlayerStatus, RoomError};

#[test]
fn new_room_reserves_player_one_for_host() {
    let host_connection = ConnectionId::new();
    let room = room(host_connection);
    let view = room.view();

    assert_eq!(view.players[0].player_index, 0);
    assert_eq!(view.players[0].status, PlayerStatus::Connected);
    assert!(view.players[0].occupied);
    assert_eq!(view.players[1].status, PlayerStatus::Empty);
}

#[test]
fn first_guest_receives_player_two() {
    let mut room = room(ConnectionId::new());
    let guest_index = room
        .join_guest(license("guest"), ConnectionId::new())
        .expect("guest joins");

    assert_eq!(guest_index, PlayerIndex::TWO);
    assert_eq!(room.status, RoomStatus::CheckingCompatibility);
}

#[test]
fn host_socket_can_attach_to_reserved_host_slot() {
    let mut room = room(ConnectionId::new());
    let host_connection = ConnectionId::new();
    let player_index = room
        .attach_host(license("host"), host_connection)
        .expect("host attaches");

    assert_eq!(player_index, PlayerIndex::ONE);
}

#[test]
fn host_attach_rejects_wrong_subject() {
    let mut room = room(ConnectionId::new());
    let result = room.attach_host(license("guest"), ConnectionId::new());

    assert!(matches!(result, Err(RoomError::HostSubjectMismatch)));
}

#[test]
fn third_player_is_rejected() {
    let mut room = room(ConnectionId::new());
    room.join_guest(license("guest"), ConnectionId::new())
        .expect("guest joins");

    assert_eq!(
        room.join_guest(license("third"), ConnectionId::new())
            .unwrap_err()
            .to_string(),
        RoomError::RoomFull.to_string()
    );
}

#[test]
fn host_disconnect_closes_room() {
    let host_connection = ConnectionId::new();
    let mut room = room(host_connection);
    let closed = room.disconnect(host_connection).expect("disconnect");

    assert!(closed);
    assert_eq!(room.view().status, RoomStatus::Closed);
}

#[test]
fn closed_room_rejects_guest_join() {
    let host_connection = ConnectionId::new();
    let mut room = room(host_connection);
    room.disconnect(host_connection).expect("disconnect");
    let result = room.join_guest(license("guest"), ConnectionId::new());

    assert!(matches!(result, Err(RoomError::RoomClosed)));
}

#[test]
fn compatibility_mismatch_blocks_ready_state() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = room(host_connection);
    room.join_guest(license("guest"), guest_connection)
        .expect("guest joins");

    room.set_compatibility_for_connection(host_connection, fingerprint("rom-a"))
        .expect("host fingerprint waits");
    let result = room.set_compatibility_for_connection(guest_connection, fingerprint("rom-b"));

    assert!(matches!(result, Err(RoomError::CompatibilityMismatch)));
}

#[test]
fn matching_compatibility_enters_syncing_state() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let room = compatible_room(host_connection, guest_connection);

    assert_eq!(room.view().status, RoomStatus::SyncingState);
}

#[test]
fn ready_from_both_players_starts_gameplay() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = compatible_room(host_connection, guest_connection);

    assert!(!room.mark_ready(host_connection).expect("host ready"));
    assert!(room.mark_ready(guest_connection).expect("guest ready"));
    assert_eq!(room.view().status, RoomStatus::Playing);
}

#[test]
fn guest_cannot_relay_snapshot_chunks() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let room = compatible_room(host_connection, guest_connection);

    let result = room.validate_snapshot_chunk(
        guest_connection,
        &SnapshotChunk {
            index: 0,
            bytes: vec![1, 2, 3],
        },
        SnapshotLimits::default(),
    );

    assert!(matches!(result, Err(RoomError::HostOnly)));
}

#[test]
fn host_can_relay_snapshot_manifest_during_sync() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let room = compatible_room(host_connection, guest_connection);

    let result = room.validate_snapshot_complete(
        host_connection,
        &SnapshotManifest {
            total_bytes: 3,
            sha256: "0".repeat(64),
        },
        SnapshotLimits::default(),
    );

    assert!(result.is_ok());
}

#[test]
fn player_cannot_send_input_for_other_slot() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = ready_room(host_connection, guest_connection);

    let result = room.accept_input_frame(
        host_connection,
        &InputFrame {
            player_index: PlayerIndex::TWO,
            frame: 0,
            payload: vec![1],
        },
        InputFrameLimits::default(),
    );

    assert!(matches!(result, Err(RoomError::SlotSpoofing(_))));
}

#[test]
fn out_of_order_frame_is_rejected() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = ready_room(host_connection, guest_connection);
    let limits = InputFrameLimits::default();

    room.accept_input_frame(host_connection, &input(PlayerIndex::ONE, 0), limits)
        .expect("first frame");
    let result = room.accept_input_frame(host_connection, &input(PlayerIndex::ONE, 0), limits);

    assert!(matches!(result, Err(RoomError::OutOfOrderFrame)));
}

#[test]
fn future_frame_limit_is_enforced() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = ready_room(host_connection, guest_connection);

    let result = room.accept_input_frame(
        host_connection,
        &input(PlayerIndex::ONE, 99),
        InputFrameLimits {
            max_future_frame_distance: 3,
        },
    );

    assert!(matches!(result, Err(RoomError::FutureFrameTooLarge)));
}

fn ready_room(host_connection: ConnectionId, guest_connection: ConnectionId) -> NetplayRoom {
    let mut room = compatible_room(host_connection, guest_connection);
    room.mark_ready(host_connection).expect("host ready");
    room.mark_ready(guest_connection).expect("guest ready");
    room
}

fn compatible_room(host_connection: ConnectionId, guest_connection: ConnectionId) -> NetplayRoom {
    let mut room = room(host_connection);
    room.join_guest(license("guest"), guest_connection)
        .expect("guest joins");
    room.set_compatibility_for_connection(host_connection, fingerprint("rom"))
        .expect("host fingerprint");
    room.set_compatibility_for_connection(guest_connection, fingerprint("rom"))
        .expect("guest fingerprint");
    room
}

fn room(host_connection: ConnectionId) -> NetplayRoom {
    NetplayRoom::new(
        license("host"),
        host_connection,
        InviteCode::parse("AB23-CD").expect("invite code"),
    )
}

fn input(player_index: PlayerIndex, frame: u64) -> InputFrame {
    InputFrame {
        player_index,
        frame,
        payload: vec![0],
    }
}

fn license(subject_id: &str) -> VerifiedLicense {
    VerifiedLicense::new(subject_id, "premium", vec!["netplay".to_string()])
}

fn fingerprint(content_hash: &str) -> CompatibilityFingerprint {
    CompatibilityFingerprint {
        desktop_version: "0.2.10".to_string(),
        protocol_version: 1,
        system_id: "n64".to_string(),
        core_id: "mupen64plus-next".to_string(),
        core_build: "core-build".to_string(),
        content_hash: content_hash.to_string(),
        settings_hash: "settings".to_string(),
        cheats_hash: "cheats".to_string(),
        system_data_hash: None,
        save_data_mode: "netplay".to_string(),
    }
}
