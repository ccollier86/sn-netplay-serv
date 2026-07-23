//! Tests for link-cable behavior in the room registry.
//!
//! These tests stay separate from the controller-netplay registry tests so link
//! packet behavior can grow without turning one test module into a god file.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    GbaSioMultiEvent, GbaSioMultiFrame, LEGACY_NETPLAY_PROTOCOL_VERSION, LinkCableCompatibility,
    LinkCableMode, LinkCablePacket, LinkCableWireHeader, NetplaySessionDescriptor,
    encode_gba_sio_multi_frame,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, LinkCableAttachment, LinkCableDataPlaneEvent,
    LinkCableDataPlaneStatus, PlayerIndex, RoomError, RoomEvent, RoomStatus,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::TryRecvError;
use tokio::time::timeout;

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
        .set_link_cable_compatibility(
            invite.clone(),
            host_connection,
            link_compatibility("android-mgba-0.10.5-sb1"),
        )
        .await
        .expect("host compatibility");
    let view = registry
        .set_link_cable_compatibility(
            invite,
            guest_connection,
            link_compatibility("android-mgba-0.10.5-sb1"),
        )
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
            link_compatibility("android-mgba-0.10.5-sb1"),
        )
        .await
        .expect("host compatibility");
    let mut events = registry.subscribe(invite.clone()).await.expect("events");

    let result = registry
        .set_link_cable_compatibility(
            invite,
            guest_connection,
            link_compatibility("android-mgba-0.10.5-sb2"),
        )
        .await;
    let event = events.recv().await.expect("event");

    assert!(matches!(result, Err(RoomError::CompatibilityMismatch)));
    assert!(matches!(event, RoomEvent::RoomStateChanged(_)));
}

#[tokio::test]
async fn validated_link_packet_uses_only_the_targeted_private_data_plane() {
    let (registry, invite, host_connection, guest_connection) = compatible_link_room().await;
    let LinkCableAttachment {
        receiver: mut host_receiver,
        snapshot: host_snapshot,
    } = registry
        .claim_link_cable_data_plane(invite.clone(), host_connection)
        .await
        .expect("host data plane")
        .expect("link room");
    let LinkCableAttachment {
        receiver: mut guest_receiver,
        snapshot: active_snapshot,
    } = registry
        .claim_link_cable_data_plane(invite.clone(), guest_connection)
        .await
        .expect("guest data plane")
        .expect("link room");

    assert_eq!(host_snapshot.status, LinkCableDataPlaneStatus::Waiting);
    assert_eq!(active_snapshot.status, LinkCableDataPlaneStatus::Active);
    assert_eq!(active_snapshot.local_slot, PlayerIndex::TWO);

    let activated = timeout(Duration::from_millis(100), host_receiver.recv())
        .await
        .expect("host activation notification")
        .expect("host private event");
    assert!(matches!(
        activated,
        LinkCableDataPlaneEvent::Lifecycle(snapshot)
            if snapshot.status == LinkCableDataPlaneStatus::Active
                && snapshot.cable_epoch == active_snapshot.cable_epoch
    ));

    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    let playing = registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    assert_eq!(playing.status, RoomStatus::Playing);
    assert_eq!(playing.room_epoch, active_snapshot.room_epoch);
    assert_eq!(playing.session_epoch, active_snapshot.session_epoch);

    let mut events = registry.subscribe(invite.clone()).await.expect("events");
    let view_before = registry
        .room_view(invite.clone())
        .await
        .expect("room before relay");
    let debug_before = registry
        .room_events(invite.clone(), 100)
        .await
        .expect("debug events before relay");
    let packet = gba_mode_set_packet(PlayerIndex::ONE, 0, 64, active_snapshot);

    registry
        .relay_link_cable_packet(
            invite.clone(),
            host_connection,
            active_snapshot.room_epoch,
            active_snapshot.session_epoch,
            packet.clone(),
        )
        .await
        .expect("link packet");

    let target_event = timeout(Duration::from_millis(100), guest_receiver.recv())
        .await
        .expect("target packet delivery")
        .expect("guest private event");
    assert_eq!(target_event, LinkCableDataPlaneEvent::Packet(packet));
    assert!(
        timeout(Duration::from_millis(20), host_receiver.recv())
            .await
            .is_err(),
        "the sender must not receive its own link packet"
    );

    let view_after = registry
        .room_view(invite.clone())
        .await
        .expect("room after relay");
    let debug_after = registry
        .room_events(invite, 100)
        .await
        .expect("debug events after relay");

    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(view_after.event_seq, view_before.event_seq);
    assert_eq!(debug_after, debug_before);
}

#[tokio::test]
async fn stale_link_route_epoch_is_rejected_without_any_delivery() {
    let (registry, invite, host_connection, guest_connection) = compatible_link_room().await;
    let LinkCableAttachment {
        receiver: mut host_receiver,
        ..
    } = registry
        .claim_link_cable_data_plane(invite.clone(), host_connection)
        .await
        .expect("host data plane")
        .expect("link room");
    let LinkCableAttachment {
        receiver: mut guest_receiver,
        snapshot: active_snapshot,
    } = registry
        .claim_link_cable_data_plane(invite.clone(), guest_connection)
        .await
        .expect("guest data plane")
        .expect("link room");
    let activated = timeout(Duration::from_millis(100), host_receiver.recv())
        .await
        .expect("host activation notification")
        .expect("host private event");
    assert!(matches!(
        activated,
        LinkCableDataPlaneEvent::Lifecycle(snapshot)
            if snapshot.status == LinkCableDataPlaneStatus::Active
    ));

    registry
        .mark_ready(invite.clone(), host_connection, None)
        .await
        .expect("host ready");
    registry
        .mark_ready(invite.clone(), guest_connection, None)
        .await
        .expect("guest ready");
    let mut events = registry.subscribe(invite.clone()).await.expect("events");
    let view_before = registry
        .room_view(invite.clone())
        .await
        .expect("room before rejected relay");
    let packet = gba_mode_set_packet(PlayerIndex::ONE, 0, 64, active_snapshot);

    let result = registry
        .relay_link_cable_packet(
            invite.clone(),
            host_connection,
            active_snapshot
                .room_epoch
                .checked_add(1)
                .expect("test room epoch"),
            active_snapshot.session_epoch,
            packet,
        )
        .await;

    assert!(matches!(result, Err(RoomError::StaleRoomEpoch)));
    assert!(
        timeout(Duration::from_millis(20), guest_receiver.recv())
            .await
            .is_err(),
        "a stale route must not reach its target"
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let view_after = registry
        .room_view(invite)
        .await
        .expect("room after rejected relay");
    assert_eq!(view_after.event_seq, view_before.event_seq);
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
        .set_link_cable_compatibility(
            invite.clone(),
            host_connection,
            link_compatibility("android-mgba-0.10.5-sb1"),
        )
        .await;
    let view = registry.room_view(invite).await.expect("room view");

    assert!(matches!(result, Err(RoomError::RoomNotReady)));
    assert_eq!(view.status, RoomStatus::Playing);
}

async fn compatible_link_room() -> (InMemoryRoomRegistry, InviteCode, ConnectionId, ConnectionId) {
    let (registry, invite, host_connection, guest_connection) = joined_link_room().await;

    registry
        .set_link_cable_compatibility(
            invite.clone(),
            host_connection,
            link_compatibility("android-mgba-0.10.5-sb1"),
        )
        .await
        .expect("host compatibility");
    registry
        .set_link_cable_compatibility(
            invite.clone(),
            guest_connection,
            link_compatibility("android-mgba-0.10.5-sb1"),
        )
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
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_connection,
            crate::rooms::ClientTransportCapabilities::default(),
        )
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
            "linkProtocol": "gba-sio-multi-v1",
            "runtimeProfile": "mgba-link-runtime-v1",
            "maxPlayers": 2
        }
    }))
    .expect("link descriptor")
}

fn link_compatibility(core_build_id: &str) -> LinkCableCompatibility {
    LinkCableCompatibility {
        protocol_version: LEGACY_NETPLAY_PROTOCOL_VERSION,
        system_family: "gba".to_string(),
        link_protocol: "gba-sio-multi-v1".to_string(),
        runtime_profile: "mgba-link-runtime-v1".to_string(),
        core_build_id: core_build_id.to_string(),
        supported_modes: vec![LinkCableMode::Multi],
    }
}

fn gba_mode_set_packet(
    player_index: PlayerIndex,
    sequence: u64,
    emulated_time: u64,
    snapshot: crate::rooms::LinkCableDataPlaneSnapshot,
) -> LinkCablePacket {
    let payload = encode_gba_sio_multi_frame(&GbaSioMultiFrame {
        header: LinkCableWireHeader {
            room_epoch: snapshot.room_epoch,
            session_epoch: snapshot.session_epoch,
            cable_epoch: snapshot.cable_epoch,
            sender_sequence: sequence,
            sender_slot: player_index.zero_based(),
        },
        event: GbaSioMultiEvent::ModeSet {
            mode: 0,
            siocnt: 0,
            rcnt: 0,
            emulated_time,
        },
    })
    .expect("valid GBA MODE_SET packet");

    LinkCablePacket {
        player_index,
        sequence,
        emulated_time,
        payload,
    }
}
