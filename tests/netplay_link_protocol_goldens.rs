//! Frozen private link-cable control-lane JSON contracts.
//!
//! These fixtures pin the endpoint-specific grant envelope separately from the
//! public room view. In particular, private room scope must remain an exact
//! decimal string and link rooms must never receive a controller input token.

use sb_netplay_serv::protocol::{
    LinkCableDataPlaneGrant, LinkCableGrantFailureReason, LinkCableGrantStatus,
    NetplayProtocolView, NetplaySessionDescriptor, ServerMessage,
};
use sb_netplay_serv::rooms::{
    PlayerFrameCursorView, PlayerRole, PlayerRuntimeState, PlayerSlotView, PlayerStatus,
    RoomFrameClockView, RoomId, RoomStatus, RoomView,
};
use serde_json::{Value, json};

const GOLDENS: &str = include_str!("fixtures/link-cable-control-goldens.json");
const ROOM_SCOPE: &str = "9223372036854775807";

#[test]
fn link_control_plane_matches_frozen_grant_lifecycle_json() {
    let fixtures: Value = serde_json::from_str(GOLDENS).expect("link control golden JSON");
    let room = link_room_view();
    let serialized_room = serde_json::to_value(&room).expect("serialize link room");

    let room_joined = ServerMessage::RoomJoined {
        event_seq: 7,
        room_epoch: 3,
        session_epoch: 5,
        your_player_index: 0,
        resume_token: "resume-token".to_string(),
        input_socket_token: None,
        voice: None,
        link_cable_grant: Some(grant(LinkCableGrantStatus::WaitingForPeer, 0, None)),
        room,
    };
    let serialized_join = serde_json::to_value(room_joined).expect("serialize link roomJoined");
    assert_golden_message(
        &serialized_join,
        &fixtures["roomJoinedWaitingForPeer"],
        &serialized_room,
    );
    assert!(
        serialized_join.get("inputSocketToken").is_none(),
        "link roomJoined must omit the controller input capability"
    );
    assert_eq!(serialized_join["voice"], Value::Null);
    assert_exact_decimal_scope(&serialized_join["linkCableGrant"]["roomScope"]);

    let updates = [
        ("ready", LinkCableGrantStatus::Ready, 9, None),
        (
            "abortedProviderReset",
            LinkCableGrantStatus::Aborted,
            9,
            Some(LinkCableGrantFailureReason::ProviderReset),
        ),
        (
            "abortedPeerDisconnected",
            LinkCableGrantStatus::Aborted,
            9,
            Some(LinkCableGrantFailureReason::PeerDisconnected),
        ),
        (
            "abortedQueueOverflow",
            LinkCableGrantStatus::Aborted,
            9,
            Some(LinkCableGrantFailureReason::QueueOverflow),
        ),
        (
            "abortedProtocolViolation",
            LinkCableGrantStatus::Aborted,
            9,
            Some(LinkCableGrantFailureReason::ProtocolViolation),
        ),
        (
            "abortedRouteClosed",
            LinkCableGrantStatus::Aborted,
            9,
            Some(LinkCableGrantFailureReason::RouteClosed),
        ),
        (
            "closed",
            LinkCableGrantStatus::Closed,
            9,
            Some(LinkCableGrantFailureReason::RouteClosed),
        ),
    ];

    for (fixture_name, status, cable_epoch, failure_reason) in updates {
        let message = ServerMessage::LinkCableGrantUpdated {
            grant: grant(status, cable_epoch, failure_reason),
        };
        let serialized = serde_json::to_value(message).expect("serialize link grant update");
        assert_eq!(
            serialized, fixtures["linkCableGrantUpdated"][fixture_name],
            "{fixture_name}: frozen link grant JSON changed"
        );
        assert_exact_decimal_scope(&serialized["grant"]["roomScope"]);
    }
}

fn grant(
    status: LinkCableGrantStatus,
    cable_epoch: u64,
    failure_reason: Option<LinkCableGrantFailureReason>,
) -> LinkCableDataPlaneGrant {
    LinkCableDataPlaneGrant {
        contract_version: 1,
        room_scope: ROOM_SCOPE.to_string(),
        room_epoch: 3,
        session_epoch: 5,
        cable_epoch,
        local_slot: 0,
        link_protocol: "gba-sio-multi-v1".to_string(),
        maximum_event_bytes: 128,
        queue_capacity: 64,
        status,
        failure_reason,
    }
}

fn link_room_view() -> RoomView {
    let session: NetplaySessionDescriptor = serde_json::from_value(json!({
        "hostClientKind": "android",
        "hostAppVersion": "0.3.0",
        "mode": "linkCable",
        "game": {
            "systemId": "gba",
            "title": "Pokemon Ruby",
            "romSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "contentKey": "gba-pokemon-ruby"
        },
        "core": {
            "coreId": "mgba",
            "coreName": "mGBA",
            "coreVersion": "android-mgba-0.10.5-sb1",
            "coreOptionsSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        },
        "link": {
            "systemFamily": "gba",
            "linkProtocol": "gba-sio-multi-v1",
            "runtimeProfile": "mgba-link-runtime-v1",
            "maxPlayers": 2,
            "transport": "relay"
        }
    }))
    .expect("decode valid link session fixture");

    RoomView {
        room_id: RoomId::new(),
        event_seq: 7,
        room_epoch: 3,
        session_epoch: 5,
        invite_code: "AB23-CD".to_string(),
        protocol: NetplayProtocolView::for_room(4),
        session,
        voice: None,
        rom_relay: None,
        max_players: 2,
        pause: None,
        state_recovery: None,
        frame_clock: RoomFrameClockView {
            canonical_frame: 0,
            released_frame: None,
            next_release_frame: 0,
            accepted_inputs: vec![PlayerFrameCursorView {
                player_index: 0,
                frame: None,
            }],
            pending_input_delay_change: None,
        },
        status: RoomStatus::WaitingForGuest,
        players: vec![PlayerSlotView {
            player_index: 0,
            display_number: 1,
            role: PlayerRole::Host,
            status: PlayerStatus::Connected,
            runtime_state: PlayerRuntimeState::Connected,
            occupied: true,
            control_connected: true,
            input_connected: false,
            supports_state_file_relay: false,
            supports_rom_file_relay: false,
            supports_scheduled_start: false,
            supports_clock_sync: false,
            supports_fast_input_relay: false,
            last_seen_age_ms: Some(11),
            reconnect_grace_remaining_ms: None,
        }],
    }
}

fn assert_golden_message(actual: &Value, expected: &Value, room: &Value) {
    let mut expanded = expected.clone();
    assert_eq!(expanded["room"], "$roomView");
    expanded["room"] = room.clone();
    assert_eq!(
        actual, &expanded,
        "frozen link roomJoined JSON contract changed"
    );
}

fn assert_exact_decimal_scope(value: &Value) {
    assert!(value.is_string(), "roomScope must remain a JSON string");
    assert_eq!(value, ROOM_SCOPE);
    assert_eq!(
        value
            .as_str()
            .expect("string checked above")
            .parse::<u64>()
            .expect("decimal room scope"),
        i64::MAX as u64
    );
}
