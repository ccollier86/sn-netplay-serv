//! Fast binary input tests for the room registry.
//!
//! These tests cover the optional `SBI2` input path. Legacy `SBI1` behavior
//! stays covered by the main registry and protocol smoke suites.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    CompatibilityFingerprint, FastInputFrame, LEGACY_NETPLAY_PROTOCOL_VERSION,
    NetplaySessionDescriptor, SnapshotChunk, SnapshotManifest, decode_fast_input_batch,
    encode_fast_input_frame,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex, RoomError, RoomInputEvent,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

struct StaticInviteCodeGenerator;

impl InviteCodeGenerator for StaticInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        InviteCode::parse("AB23-CD").expect("invite")
    }
}

#[tokio::test]
async fn fast_input_requires_every_player_to_advertise_fast_relay() {
    let (registry, invite, host_connection, _guest_connection) = configured_room(false).await;
    complete_snapshot(&registry, &invite, host_connection).await;
    let batch = fast_input_batch(1, 1, PlayerIndex::ONE, 0, &[1]);

    let result = registry
        .relay_fast_input_batch(invite, host_connection, batch)
        .await;

    assert!(matches!(result, Err(RoomError::RoomNotReady)));
}

#[tokio::test]
async fn fast_input_relay_preserves_encoded_record_for_peer_fanout() {
    let (registry, invite, host_connection, guest_connection) = configured_room(true).await;
    complete_snapshot(&registry, &invite, host_connection).await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let mut input_events = registry
        .subscribe_input(invite.clone())
        .await
        .expect("input events");
    let room = registry.room_view(invite.clone()).await.expect("room");
    let encoded = encode_fast_input_frame(
        room.room_epoch,
        room.session_epoch,
        PlayerIndex::ONE,
        0,
        &[8, 9],
    )
    .expect("encoded fast input");

    registry
        .relay_fast_input_batch(
            invite.clone(),
            host_connection,
            decode_fast_input_batch(encoded.clone()).expect("decoded fast input"),
        )
        .await
        .expect("relay fast input");
    registry.release_next_controller_frames().await;

    let fast_frame = receive_fast_input_frame(&mut input_events).await;

    assert_eq!(fast_frame.encoded(), encoded);
}

async fn configured_room(
    supports_fast_input_relay: bool,
) -> (InMemoryRoomRegistry, InviteCode, ConnectionId, ConnectionId) {
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
            fast_input_capabilities(supports_fast_input_relay),
        )
        .await
        .expect("host");
    let guest_join = registry
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_connection,
            fast_input_capabilities(supports_fast_input_relay),
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
        host_join
            .input_socket_token
            .as_deref()
            .expect("controller input token"),
        guest_connection,
        guest_join
            .input_socket_token
            .as_deref()
            .expect("controller input token"),
    )
    .await;

    (registry, invite, host_connection, guest_connection)
}

fn fast_input_capabilities(
    supports_fast_input_relay: bool,
) -> crate::rooms::ClientTransportCapabilities {
    crate::rooms::ClientTransportCapabilities {
        supports_fast_input_relay,
        ..crate::rooms::ClientTransportCapabilities::default()
    }
}

async fn receive_fast_input_frame(
    events: &mut crate::rooms::RoomInputEventReceiver,
) -> FastInputFrame {
    for _ in 0..4 {
        if let RoomInputEvent::FastInputFrame { frame, .. } =
            events.recv().await.expect("input event")
        {
            return frame;
        }
    }

    panic!("fast input frame was not emitted");
}

fn fast_input_batch(
    room_epoch: u64,
    session_epoch: u64,
    player_index: PlayerIndex,
    frame: u64,
    payload: &[u8],
) -> crate::protocol::FastInputBatch {
    decode_fast_input_batch(
        encode_fast_input_frame(room_epoch, session_epoch, player_index, frame, payload)
            .expect("encoded fast input"),
    )
    .expect("decoded fast input")
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
