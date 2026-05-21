//! Tests for link-cable behavior in the room registry.
//!
//! These tests stay separate from the controller-netplay registry tests so link
//! packet behavior can grow without turning one test module into a god file.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    LinkCableCompatibility, LinkCablePacket, NETPLAY_PROTOCOL_VERSION, NetplaySessionDescriptor,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex, RoomError, RoomEvent, RoomStatus,
};
use std::sync::Arc;

struct StaticInviteCodeGenerator;

impl InviteCodeGenerator for StaticInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        InviteCode::parse("AB23-CD").expect("invite")
    }
}

#[tokio::test]
async fn link_compatibility_enters_syncing_state() {
    let (registry, invite, host_connection, guest_connection) = joined_link_room().await;

    registry
        .set_link_cable_compatibility(invite.clone(), host_connection, link_compatibility(None))
        .await
        .expect("host compatibility");
    let view = registry
        .set_link_cable_compatibility(invite, guest_connection, link_compatibility(None))
        .await
        .expect("guest compatibility");

    assert_eq!(view.status, RoomStatus::SyncingState);
}

#[tokio::test]
async fn link_compatibility_mismatch_broadcasts_room_state() {
    let (registry, invite, host_connection, guest_connection) = joined_link_room().await;
    registry
        .set_link_cable_compatibility(
            invite.clone(),
            host_connection,
            link_compatibility(Some("bios-a")),
        )
        .await
        .expect("host compatibility");
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    let result = registry
        .set_link_cable_compatibility(invite, guest_connection, link_compatibility(Some("bios-b")))
        .await;
    let event = events.recv().await.expect("event");

    assert!(matches!(result, Err(RoomError::CompatibilityMismatch)));
    assert!(matches!(event, RoomEvent::RoomStateChanged(_)));
}

#[tokio::test]
async fn validated_link_packet_is_broadcast() {
    let (registry, invite, host_connection, guest_connection) = compatible_link_room().await;
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
        .relay_link_cable_packet(invite, host_connection, link_packet(PlayerIndex::ONE, 1))
        .await
        .expect("link packet");

    let event = events.recv().await.expect("event");

    assert!(matches!(event, RoomEvent::LinkCablePacket { .. }));
}

#[tokio::test]
async fn link_compatibility_after_start_does_not_roll_back_room() {
    let (registry, invite, host_connection, guest_connection) = compatible_link_room().await;
    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");

    let result = registry
        .set_link_cable_compatibility(invite.clone(), host_connection, link_compatibility(None))
        .await;
    let view = registry.room_view(invite).await.expect("room view");

    assert!(matches!(result, Err(RoomError::RoomNotReady)));
    assert_eq!(view.status, RoomStatus::Playing);
}

async fn compatible_link_room() -> (InMemoryRoomRegistry, InviteCode, ConnectionId, ConnectionId) {
    let (registry, invite, host_connection, guest_connection) = joined_link_room().await;

    registry
        .set_link_cable_compatibility(invite.clone(), host_connection, link_compatibility(None))
        .await
        .expect("host compatibility");
    registry
        .set_link_cable_compatibility(invite.clone(), guest_connection, link_compatibility(None))
        .await
        .expect("guest compatibility");

    (registry, invite, host_connection, guest_connection)
}

async fn joined_link_room() -> (InMemoryRoomRegistry, InviteCode, ConnectionId, ConnectionId) {
    let registry = registry();
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let view = registry
        .create_room(license("host"), host_connection, link_descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");

    registry
        .connect_guest(invite.clone(), license("guest"), guest_connection)
        .await
        .expect("guest");

    (registry, invite, host_connection, guest_connection)
}

fn registry() -> InMemoryRoomRegistry {
    InMemoryRoomRegistry::new(Arc::new(StaticInviteCodeGenerator))
}

fn license(subject_id: &str) -> VerifiedLicense {
    VerifiedLicense::new(subject_id, "premium", vec!["netplay".to_string()])
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
