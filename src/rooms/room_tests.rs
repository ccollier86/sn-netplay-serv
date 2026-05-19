//! Tests for room domain behavior.
//!
//! Keeping these in a sibling module keeps the production room model readable
//! while still testing the security-sensitive slot and frame rules directly.

use super::{NetplayRoom, RoomStatus};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    CompatibilityFingerprint, InputFrame, InputFrameLimits, LinkCableCompatibility,
    LinkCablePacket, LinkCablePacketLimits, NETPLAY_PROTOCOL_VERSION, NetplaySessionDescriptor,
    SessionPauseReason, SessionPauseState, SnapshotChunk, SnapshotLimits, SnapshotManifest,
};
use crate::rooms::{
    ConnectionId, InputFrameAcceptance, InviteCode, PlayerIndex, PlayerStatus, RoomError,
    SessionResumeOutcome,
};
use sha2::{Digest, Sha256};

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
fn fingerprint_must_match_room_descriptor() {
    let host_connection = ConnectionId::new();
    let mut room = room(host_connection);

    let result = room.set_compatibility_for_connection(host_connection, fingerprint("rom-b"));

    assert!(matches!(result, Err(RoomError::CompatibilityMismatch)));
    assert_eq!(
        room.view().players[0].status,
        PlayerStatus::CompatibilityFailed
    );
}

#[test]
fn fingerprint_state_format_must_match_room_descriptor() {
    let host_connection = ConnectionId::new();
    let mut room = room(host_connection);
    let mut incompatible_fingerprint = fingerprint("rom");

    incompatible_fingerprint.state_format = Some("dolphin:gamecube:other-v1".to_string());
    let result = room.set_compatibility_for_connection(host_connection, incompatible_fingerprint);

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
fn compatibility_hash_case_does_not_create_false_mismatch() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = room(host_connection);
    room.join_guest(license("guest"), guest_connection)
        .expect("guest joins");

    room.set_compatibility_for_connection(host_connection, fingerprint(&"A".repeat(64)))
        .expect("host fingerprint waits");
    room.set_compatibility_for_connection(guest_connection, fingerprint(&"a".repeat(64)))
        .expect("guest fingerprint");

    assert_eq!(room.view().status, RoomStatus::SyncingState);
}

#[test]
fn controller_ready_requires_completed_host_snapshot() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = compatible_room(host_connection, guest_connection);

    let result = room.mark_ready(host_connection);

    assert!(matches!(result, Err(RoomError::RoomNotReady)));
    assert_eq!(room.view().status, RoomStatus::SyncingState);
}

#[test]
fn ready_from_both_players_starts_gameplay() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = compatible_room(host_connection, guest_connection);
    complete_snapshot(&mut room, host_connection);
    attach_input_sockets(&mut room, host_connection, guest_connection);

    assert!(!room.mark_ready(host_connection).expect("host ready"));
    assert!(room.mark_ready(guest_connection).expect("guest ready"));
    assert_eq!(room.view().status, RoomStatus::Playing);
}

#[test]
fn guest_cannot_relay_snapshot_chunks() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = compatible_room(host_connection, guest_connection);

    let result = room.accept_snapshot_chunk(
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
    let mut room = compatible_room(host_connection, guest_connection);

    room.accept_snapshot_chunk(
        host_connection,
        &SnapshotChunk {
            index: 0,
            bytes: vec![1, 2, 3],
        },
        SnapshotLimits::default(),
    )
    .expect("chunk");

    let result = room.accept_snapshot_complete(
        host_connection,
        &snapshot_manifest(&[1, 2, 3]),
        SnapshotLimits::default(),
    );

    assert!(result.is_ok());
}

#[test]
fn snapshot_manifest_must_match_relayed_chunks() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = compatible_room(host_connection, guest_connection);

    room.accept_snapshot_chunk(
        host_connection,
        &SnapshotChunk {
            index: 0,
            bytes: vec![1, 2, 3],
        },
        SnapshotLimits::default(),
    )
    .expect("chunk");

    let result = room.accept_snapshot_complete(
        host_connection,
        &SnapshotManifest {
            total_bytes: 3,
            sha256: "0".repeat(64),
        },
        SnapshotLimits::default(),
    );

    assert!(matches!(result, Err(RoomError::SnapshotInvalid)));
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

#[test]
fn link_compatibility_enters_syncing_state() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = link_room(host_connection);
    room.join_guest(license("guest"), guest_connection)
        .expect("guest joins");

    room.set_link_cable_compatibility_for_connection(host_connection, link_compatibility(None))
        .expect("host link compatibility");
    room.set_link_cable_compatibility_for_connection(guest_connection, link_compatibility(None))
        .expect("guest link compatibility");

    assert_eq!(room.view().status, RoomStatus::SyncingState);
}

#[test]
fn link_compatibility_rejects_mismatched_system_data() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = link_room(host_connection);
    room.join_guest(license("guest"), guest_connection)
        .expect("guest joins");

    room.set_link_cable_compatibility_for_connection(
        host_connection,
        link_compatibility(Some("bios-a")),
    )
    .expect("host link compatibility");
    let result = room.set_link_cable_compatibility_for_connection(
        guest_connection,
        link_compatibility(Some("bios-b")),
    );

    assert!(matches!(result, Err(RoomError::CompatibilityMismatch)));
}

#[test]
fn link_packets_relay_only_after_link_room_is_playing() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = compatible_link_room(host_connection, guest_connection);
    let packet = link_packet(PlayerIndex::ONE, 1);

    assert!(matches!(
        room.accept_link_cable_packet(host_connection, &packet, LinkCablePacketLimits::default(),),
        Err(RoomError::NotPlaying)
    ));

    room.mark_ready(host_connection).expect("host ready");
    room.mark_ready(guest_connection).expect("guest ready");

    assert!(
        room.accept_link_cable_packet(host_connection, &packet, LinkCablePacketLimits::default(),)
            .is_ok()
    );
}

#[test]
fn link_packet_cannot_spoof_player_slot() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = ready_link_room(host_connection, guest_connection);

    let result = room.accept_link_cable_packet(
        host_connection,
        &link_packet(PlayerIndex::TWO, 1),
        LinkCablePacketLimits::default(),
    );

    assert!(matches!(result, Err(RoomError::SlotSpoofing(_))));
}

#[test]
fn coordinated_pause_requires_all_players_before_paused() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = ready_room(host_connection, guest_connection);

    let pause = room
        .request_session_pause(host_connection, SessionPauseReason::Menu, 10)
        .expect("pause scheduled");
    assert_eq!(pause.sequence, 1);
    assert_eq!(pause.pause_at_frame, 18);
    assert_eq!(
        room.view().pause.expect("pause view").state,
        SessionPauseState::Pausing
    );

    room.mark_session_pause_reached(host_connection, pause.sequence, 18)
        .expect("host paused");
    assert_eq!(room.status(), RoomStatus::Playing);

    let paused = room
        .mark_session_pause_reached(guest_connection, pause.sequence, 18)
        .expect("guest paused");
    assert_eq!(paused.paused_at_frame, Some(18));
    assert_eq!(room.status(), RoomStatus::Paused);
    assert_eq!(room.view().players[0].status, PlayerStatus::Paused);
}

#[test]
fn coordinated_pause_allows_in_flight_delayed_input_without_error() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = ready_room(host_connection, guest_connection);

    let pause = room
        .request_session_pause(host_connection, SessionPauseReason::Menu, 10)
        .expect("pause scheduled");

    assert_eq!(
        room.accept_input_frame(
            host_connection,
            &input(PlayerIndex::ONE, pause.pause_at_frame + 3),
            InputFrameLimits {
                max_future_frame_distance: 32,
            },
        )
        .expect("in-flight input accepted"),
        InputFrameAcceptance::Relay
    );
    assert_eq!(
        room.accept_input_frame(
            guest_connection,
            &input(PlayerIndex::TWO, pause.pause_at_frame + 4),
            InputFrameLimits {
                max_future_frame_distance: 32,
            },
        )
        .expect("post-window input ignored"),
        InputFrameAcceptance::Ignore
    );
}

#[test]
fn coordinated_resume_waits_for_all_pause_holders_to_release() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = ready_room(host_connection, guest_connection);

    let pause = room
        .request_session_pause(host_connection, SessionPauseReason::Menu, 10)
        .expect("pause scheduled");
    room.request_session_pause(guest_connection, SessionPauseReason::Menu, 10)
        .expect("guest holds pause");
    room.mark_session_pause_reached(host_connection, pause.sequence, 18)
        .expect("host paused");
    room.mark_session_pause_reached(guest_connection, pause.sequence, 18)
        .expect("guest paused");

    assert!(matches!(
        room.request_session_resume(host_connection, pause.sequence),
        Ok(SessionResumeOutcome::StillPaused(_))
    ));
    assert_eq!(room.status(), RoomStatus::Paused);

    assert!(matches!(
        room.request_session_resume(guest_connection, pause.sequence),
        Ok(SessionResumeOutcome::Resumed {
            resume_at_frame: 19
        })
    ));
    assert_eq!(room.status(), RoomStatus::Playing);
}

#[test]
fn resume_before_pause_ack_waits_then_auto_resumes_after_all_acks() {
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let mut room = ready_room(host_connection, guest_connection);

    let pause = room
        .request_session_pause(host_connection, SessionPauseReason::Menu, 10)
        .expect("pause scheduled");
    assert!(matches!(
        room.request_session_resume(host_connection, pause.sequence),
        Ok(SessionResumeOutcome::StillPaused(_))
    ));

    room.mark_session_pause_reached(host_connection, pause.sequence, 18)
        .expect("host paused");
    assert_eq!(room.status(), RoomStatus::Playing);
    let outcome = room
        .mark_session_pause_reached_with_outcome(guest_connection, pause.sequence, 18)
        .expect("guest paused");

    assert!(matches!(
        outcome,
        crate::rooms::SessionPauseReachedOutcome::Resumed {
            sequence: 1,
            resume_at_frame: 19
        }
    ));
    assert_eq!(room.status(), RoomStatus::Playing);
}

fn ready_room(host_connection: ConnectionId, guest_connection: ConnectionId) -> NetplayRoom {
    let mut room = compatible_room(host_connection, guest_connection);
    complete_snapshot(&mut room, host_connection);
    attach_input_sockets(&mut room, host_connection, guest_connection);
    room.mark_ready(host_connection).expect("host ready");
    room.mark_ready(guest_connection).expect("guest ready");
    room
}

fn ready_link_room(host_connection: ConnectionId, guest_connection: ConnectionId) -> NetplayRoom {
    let mut room = compatible_link_room(host_connection, guest_connection);
    room.mark_ready(host_connection).expect("host ready");
    room.mark_ready(guest_connection).expect("guest ready");
    room
}

fn compatible_link_room(
    host_connection: ConnectionId,
    guest_connection: ConnectionId,
) -> NetplayRoom {
    let mut room = link_room(host_connection);
    room.join_guest(license("guest"), guest_connection)
        .expect("guest joins");
    room.set_link_cable_compatibility_for_connection(host_connection, link_compatibility(None))
        .expect("host link compatibility");
    room.set_link_cable_compatibility_for_connection(guest_connection, link_compatibility(None))
        .expect("guest link compatibility");
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

fn attach_input_sockets(
    room: &mut NetplayRoom,
    host_input_connection: ConnectionId,
    guest_input_connection: ConnectionId,
) {
    let view = room.view();

    room.attach_input_socket(
        PlayerIndex::ONE,
        view.room_epoch,
        view.session_epoch,
        "",
        host_input_connection,
        std::time::Instant::now(),
    )
    .expect("host input socket");
    room.attach_input_socket(
        PlayerIndex::TWO,
        view.room_epoch,
        view.session_epoch,
        "",
        guest_input_connection,
        std::time::Instant::now(),
    )
    .expect("guest input socket");
}

fn complete_snapshot(room: &mut NetplayRoom, host_connection: ConnectionId) {
    room.accept_snapshot_chunk(
        host_connection,
        &SnapshotChunk {
            index: 0,
            bytes: vec![1, 2, 3],
        },
        SnapshotLimits::default(),
    )
    .expect("snapshot chunk");
    room.accept_snapshot_complete(
        host_connection,
        &snapshot_manifest(&[1, 2, 3]),
        SnapshotLimits::default(),
    )
    .expect("snapshot complete");
}

fn room(host_connection: ConnectionId) -> NetplayRoom {
    NetplayRoom::new(
        license("host"),
        host_connection,
        InviteCode::parse("AB23-CD").expect("invite code"),
        descriptor(),
    )
}

fn link_room(host_connection: ConnectionId) -> NetplayRoom {
    NetplayRoom::new(
        license("host"),
        host_connection,
        InviteCode::parse("AB23-CD").expect("invite code"),
        link_descriptor(),
    )
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
        "controller": {
            "inputDelayFrames": 3
        },
        "core": {
            "coreId": "dolphin",
            "stateFormat": "dolphin:gamecube:libretro-serialize-v1"
        }
    }))
    .expect("descriptor")
}

fn link_descriptor() -> NetplaySessionDescriptor {
    serde_json::from_value(serde_json::json!({
        "hostAppVersion": "0.3.0",
        "mode": "linkCable",
        "game": {
            "systemId": "gba",
            "title": "Pokemon Ruby",
            "romSha256": "a".repeat(64),
            "contentKey": "gba-ruby"
        },
        "core": {
            "coreId": "mgba"
        },
        "link": {
            "systemFamily": "gba",
            "linkProtocol": "gba-link-cable-v1",
            "runtimeProfile": "mgba-link-runtime-v1",
            "maxPlayers": 2
        }
    }))
    .expect("link descriptor")
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

fn link_compatibility(system_data_hash: Option<&str>) -> LinkCableCompatibility {
    LinkCableCompatibility {
        protocol_version: NETPLAY_PROTOCOL_VERSION,
        system_family: "gba".to_string(),
        link_protocol: "gba-link-cable-v1".to_string(),
        runtime_profile: "mgba-link-runtime-v1".to_string(),
        system_data_hash: system_data_hash.map(str::to_string),
    }
}

fn link_packet(player_index: PlayerIndex, sequence: u64) -> LinkCablePacket {
    LinkCablePacket {
        player_index,
        sequence,
        emulated_time: sequence * 16,
        payload: vec![1, 2, 3],
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
