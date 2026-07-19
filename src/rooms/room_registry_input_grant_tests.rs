//! Focused tests for one-time, control-bound binary input capabilities.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    CompatibilityFingerprint, LEGACY_NETPLAY_PROTOCOL_VERSION, NetplaySessionDescriptor,
    SnapshotChunk, SnapshotManifest,
};
use crate::rooms::{
    ClientTransportCapabilities, ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex,
    RoomError, RoomStatus,
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
async fn input_loss_starts_bounded_control_recovery_and_requires_fresh_grants() {
    let (registry, invite, host_join, host_input_connection) = playing_room().await;
    let pre_recovery_epoch = registry
        .room_view(invite.clone())
        .await
        .expect("playing room")
        .room_epoch;
    let original_resume_token = host_join.resume_token.clone();
    let original_input_token = host_join.input_socket_token.clone();

    let recovering = registry
        .disconnect_input_socket(invite.clone(), host_input_connection)
        .await
        .expect("input loss");
    assert_eq!(recovering.status, RoomStatus::Recovering);
    assert!(!recovering.players[0].control_connected);
    assert!(recovering.players[0].reconnect_grace_remaining_ms.is_some());

    let resumed = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::ONE,
            pre_recovery_epoch,
            original_resume_token.clone(),
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("control reconnect");
    assert_ne!(resumed.resume_token, original_resume_token);

    let stale_input = registry
        .connect_input_socket(
            invite.clone(),
            PlayerIndex::ONE,
            resumed.room.room_epoch,
            resumed.room.session_epoch,
            original_input_token,
            ConnectionId::new(),
        )
        .await;
    assert!(matches!(stale_input, Err(RoomError::ResumeTokenInvalid)));

    registry
        .connect_input_socket(
            invite.clone(),
            PlayerIndex::ONE,
            resumed.room.room_epoch,
            resumed.room.session_epoch,
            resumed.input_socket_token.clone(),
            ConnectionId::new(),
        )
        .await
        .expect("fresh input grant");
    let replay = registry
        .connect_input_socket(
            invite,
            PlayerIndex::ONE,
            resumed.room.room_epoch,
            resumed.room.session_epoch,
            resumed.input_socket_token,
            ConnectionId::new(),
        )
        .await;
    assert!(matches!(replay, Err(RoomError::ResumeTokenInvalid)));
}

#[tokio::test]
async fn input_loss_recovery_expires_without_control_reconnect() {
    let (registry, invite, _host_join, host_input_connection) = playing_room().await;

    registry
        .disconnect_input_socket(invite.clone(), host_input_connection)
        .await
        .expect("input loss");
    let removed = registry
        .remove_expired_waiting_rooms(
            Instant::now() + Duration::from_secs(91),
            Duration::from_secs(600),
        )
        .await;

    assert_eq!(removed, 1);
    assert!(matches!(
        registry.room_view(invite).await,
        Err(RoomError::NotFound)
    ));
}

async fn playing_room() -> (
    InMemoryRoomRegistry,
    InviteCode,
    crate::rooms::RoomJoin,
    ConnectionId,
) {
    let registry = InMemoryRoomRegistry::new(Arc::new(StaticInviteCodeGenerator));
    let room = registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(room.invite_code).expect("invite");
    let host_control = ConnectionId::new();
    let guest_control = ConnectionId::new();
    let host_join = registry
        .connect_host(
            invite.clone(),
            license("host"),
            host_control,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("host");
    let guest_join = registry
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_control,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest");
    let input_epoch = registry
        .room_view(invite.clone())
        .await
        .expect("input epoch");
    let host_input = ConnectionId::new();
    registry
        .connect_input_socket(
            invite.clone(),
            PlayerIndex::ONE,
            input_epoch.room_epoch,
            input_epoch.session_epoch,
            host_join.input_socket_token.clone(),
            host_input,
        )
        .await
        .expect("host input");
    registry
        .connect_input_socket(
            invite.clone(),
            PlayerIndex::TWO,
            input_epoch.room_epoch,
            input_epoch.session_epoch,
            guest_join.input_socket_token,
            ConnectionId::new(),
        )
        .await
        .expect("guest input");

    registry
        .set_compatibility(invite.clone(), host_control, fingerprint())
        .await
        .expect("host compatibility");
    registry
        .set_compatibility(invite.clone(), guest_control, fingerprint())
        .await
        .expect("guest compatibility");
    let snapshot = [1_u8, 2, 3];
    registry
        .relay_snapshot_chunk(
            invite.clone(),
            host_control,
            SnapshotChunk {
                snapshot_id: "snapshot-1".to_string(),
                repair_frame: 0,
                index: 0,
                bytes: snapshot.to_vec(),
            },
        )
        .await
        .expect("snapshot chunk");
    registry
        .relay_snapshot_complete(
            invite.clone(),
            host_control,
            SnapshotManifest {
                snapshot_id: "snapshot-1".to_string(),
                repair_frame: 0,
                total_bytes: snapshot.len() as u64,
                sha256: format!("{:x}", Sha256::digest(snapshot)),
            },
        )
        .await
        .expect("snapshot complete");
    registry
        .mark_ready(invite.clone(), host_control, None)
        .await
        .expect("host ready");
    let playing = registry
        .mark_ready(invite.clone(), guest_control, None)
        .await
        .expect("guest ready");
    assert_eq!(playing.status, RoomStatus::Playing);

    (registry, invite, host_join, host_input)
}

fn license(subject: &str) -> VerifiedLicense {
    VerifiedLicense::new(subject, "premium", vec!["netplay".to_string()])
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
        "controller": { "inputDelayFrames": 3 }
    }))
    .expect("descriptor")
}

fn fingerprint() -> CompatibilityFingerprint {
    CompatibilityFingerprint {
        desktop_version: "0.3.0".to_string(),
        protocol_version: LEGACY_NETPLAY_PROTOCOL_VERSION,
        system_id: "gamecube".to_string(),
        core_id: "dolphin".to_string(),
        core_build: "core-build".to_string(),
        state_format: Some("dolphin:gamecube:libretro-serialize-v1".to_string()),
        content_hash: "a".repeat(64),
        settings_hash: "settings".to_string(),
        cheats_hash: "cheats".to_string(),
        system_data_hash: None,
        save_data_mode: "netplay".to_string(),
        determinism_v5: None,
    }
}
