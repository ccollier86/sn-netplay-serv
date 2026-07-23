//! Frozen controller-netplay JSON contracts for the v4 and v5 control lanes.
//!
//! Link-cable providers are additive. These fixtures deliberately exercise the
//! existing controller descriptors and every room-bearing control envelope so
//! provider metadata cannot silently leak into controller room payloads.

use sb_netplay_serv::protocol::{
    ClientMessage, NetplayClientKind, NetplayProtocolView, NetplaySessionDescriptor,
    ScheduledSessionStart, ServerMessage, SessionPauseHolder, SessionPauseReason,
    SessionPauseState, SessionPauseView,
};
use sb_netplay_serv::rooms::{
    PlayerFrameCursorView, PlayerIndex, PlayerRole, PlayerRuntimeState, PlayerSlotView,
    PlayerStatus, RoomFrameClockView, RoomId, RoomStatus, RoomView,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const GOLDENS: &str = include_str!("fixtures/controller-room-protocol-goldens.json");

#[test]
fn controller_v4_control_plane_matches_frozen_json_contract() {
    assert_controller_contract("v4", 4);
}

#[test]
fn controller_v5_control_plane_matches_frozen_json_contract() {
    assert_controller_contract("v5", 5);
}

fn assert_controller_contract(fixture_name: &str, protocol_version: u16) {
    let fixtures: Value = serde_json::from_str(GOLDENS).expect("controller golden JSON");
    let fixture = &fixtures[fixture_name];
    assert!(fixture.is_object(), "missing {fixture_name} golden fixture");

    let mut session: NetplaySessionDescriptor =
        serde_json::from_value(fixture["createRequest"]["session"].clone())
            .expect("decode controller create session");
    session.host_client_kind = Some(NetplayClientKind::Desktop);
    session
        .validate()
        .expect("golden controller session remains valid");

    let serialized_session = serde_json::to_value(&session).expect("serialize controller session");
    assert_json_golden(
        &serialized_session,
        &fixture["session"],
        &format!("{fixture_name}.session"),
    );
    assert_eq!(serialized_session["mode"], "controllerNetplay");
    assert_eq!(serialized_session["link"], Value::Null);

    assert_client_message_goldens(fixture_name, fixture, protocol_version);

    let room = controller_room_view(protocol_version, session);
    let serialized_room = serde_json::to_value(&room).expect("serialize controller room view");
    assert_json_golden(
        &serialized_room,
        &fixture["roomView"],
        &format!("{fixture_name}.roomView"),
    );

    let create_response = json!({ "room": serialized_room });
    let create_expected =
        expand_room_fixture(&fixture["serverMessages"]["createResponse"], fixture);
    assert_json_golden(
        &create_response,
        &create_expected,
        &format!("{fixture_name}.serverMessages.createResponse"),
    );

    let scheduled_start = (protocol_version >= 5).then_some(ScheduledSessionStart {
        room_epoch: 3,
        session_epoch: 5,
        start_frame: 120,
        server_time_ms: 12_400,
        created_at_server_time_ms: 12_000,
        minimum_start_delay_ms: 400,
        clock_uncertainty_budget_ms: 80,
    });
    let resume_start = (protocol_version >= 5).then_some(ScheduledSessionStart {
        room_epoch: 3,
        session_epoch: 6,
        start_frame: 127,
        server_time_ms: 15_400,
        created_at_server_time_ms: 15_000,
        minimum_start_delay_ms: 400,
        clock_uncertainty_budget_ms: 80,
    });
    let pause_scheduled = scheduled_pause();
    let pause_updated = acknowledged_pause();

    let server_messages = [
        (
            "roomJoined",
            ServerMessage::RoomJoined {
                event_seq: 17,
                room_epoch: 3,
                session_epoch: 5,
                your_player_index: 1,
                resume_token: "resume-token".to_string(),
                input_socket_token: "input-socket-token".to_string(),
                voice: None,
                room: room.clone(),
            },
        ),
        (
            "compatibilityRequested",
            ServerMessage::CompatibilityRequested {
                event_seq: 17,
                room_epoch: 3,
                session_epoch: 5,
                room: room.clone(),
            },
        ),
        (
            "compatibilityAccepted",
            ServerMessage::RoomStateChanged {
                event_seq: 17,
                room_epoch: 3,
                session_epoch: 5,
                room: room.clone(),
            },
        ),
        (
            "startSession",
            ServerMessage::StartSession {
                event_seq: 17,
                room_epoch: 3,
                session_epoch: 5,
                start_frame: 120,
                scheduled_start,
                room: room.clone(),
            },
        ),
        (
            "sessionPauseScheduled",
            ServerMessage::SessionPauseScheduled {
                event_seq: 17,
                room_epoch: 3,
                session_epoch: 5,
                pause: pause_scheduled,
                room: room.clone(),
            },
        ),
        (
            "sessionPauseUpdated",
            ServerMessage::SessionPauseUpdated {
                event_seq: 17,
                room_epoch: 3,
                session_epoch: 5,
                pause: pause_updated,
                room: room.clone(),
            },
        ),
        (
            "sessionResumeScheduled",
            ServerMessage::SessionResumeScheduled {
                event_seq: 18,
                room_epoch: 3,
                session_epoch: if protocol_version >= 5 { 6 } else { 5 },
                sequence: 4,
                resume_at_frame: 127,
                scheduled_start: resume_start,
                room: room.clone(),
            },
        ),
        (
            "recoveryStarted",
            ServerMessage::RecoveryStarted {
                event_seq: 18,
                room_epoch: 3,
                session_epoch: 5,
                room: room.clone(),
            },
        ),
        (
            "reconnectRoomJoined",
            ServerMessage::RoomJoined {
                event_seq: 18,
                room_epoch: 3,
                session_epoch: 5,
                your_player_index: 1,
                resume_token: "resume-token-next".to_string(),
                input_socket_token: "input-socket-token-next".to_string(),
                voice: None,
                room: room.clone(),
            },
        ),
        (
            "playerReconnected",
            ServerMessage::PlayerReconnected {
                event_seq: 18,
                room_epoch: 3,
                session_epoch: 5,
                player_index: 1,
                room: room.clone(),
            },
        ),
        (
            "recoveryResyncRequired",
            ServerMessage::RecoveryResyncRequired {
                event_seq: 19,
                room_epoch: 4,
                session_epoch: 6,
                room,
            },
        ),
    ];

    for (message_name, message) in server_messages {
        let actual = serde_json::to_value(message).expect("serialize controller server message");
        let expected = expand_room_fixture(&fixture["serverMessages"][message_name], fixture);
        assert_json_golden(
            &actual,
            &expected,
            &format!("{fixture_name}.serverMessages.{message_name}"),
        );
    }
}

fn assert_client_message_goldens(fixture_name: &str, fixture: &Value, protocol_version: u16) {
    let compatibility: ClientMessage =
        serde_json::from_value(fixture["clientMessages"]["setCompatibilityFingerprint"].clone())
            .expect("decode compatibility message");
    let ClientMessage::SetCompatibilityFingerprint {
        room_epoch,
        session_epoch,
        fingerprint,
    } = compatibility
    else {
        panic!("{fixture_name} compatibility fixture decoded to the wrong message");
    };
    assert_eq!((room_epoch, session_epoch), (3, 5));
    assert_eq!(fingerprint.protocol_version, protocol_version);
    assert_json_golden(
        &serde_json::to_value(&*fingerprint).expect("serialize compatibility fingerprint"),
        &fixture["compatibilityFingerprint"],
        &format!("{fixture_name}.compatibilityFingerprint"),
    );

    let pause: ClientMessage =
        serde_json::from_value(fixture["clientMessages"]["requestSessionPause"].clone())
            .expect("decode pause request");
    assert!(matches!(
        pause,
        ClientMessage::RequestSessionPause {
            room_epoch: 3,
            session_epoch: 5,
            request_id,
            reason: SessionPauseReason::Menu,
            local_frame: 120,
        } if request_id == "pause-4"
    ));

    let reached: ClientMessage =
        serde_json::from_value(fixture["clientMessages"]["sessionPauseReached"].clone())
            .expect("decode pause acknowledgement");
    assert!(matches!(
        reached,
        ClientMessage::SessionPauseReached {
            room_epoch: 3,
            session_epoch: 5,
            sequence: 4,
            paused_at_frame: 126,
        }
    ));

    let resume: ClientMessage =
        serde_json::from_value(fixture["clientMessages"]["requestSessionResume"].clone())
            .expect("decode resume request");
    assert!(matches!(
        resume,
        ClientMessage::RequestSessionResume {
            room_epoch: 3,
            session_epoch: 5,
            request_id,
            sequence: 4,
            reason: SessionPauseReason::Menu,
        } if request_id == "resume-4"
    ));
}

fn controller_room_view(protocol_version: u16, session: NetplaySessionDescriptor) -> RoomView {
    let supports_v5 = protocol_version >= 5;
    RoomView {
        room_id: RoomId::new(),
        event_seq: 17,
        room_epoch: 3,
        session_epoch: 5,
        invite_code: "AB23-CD".to_string(),
        protocol: NetplayProtocolView::for_room(protocol_version),
        session,
        voice: None,
        rom_relay: None,
        max_players: 2,
        pause: None,
        state_recovery: None,
        frame_clock: RoomFrameClockView {
            canonical_frame: 120,
            released_frame: Some(119),
            next_release_frame: 120,
            accepted_inputs: vec![
                PlayerFrameCursorView {
                    player_index: 0,
                    frame: Some(120),
                },
                PlayerFrameCursorView {
                    player_index: 1,
                    frame: Some(119),
                },
            ],
            pending_input_delay_change: None,
        },
        status: RoomStatus::Playing,
        players: vec![
            controller_player(0, 1, PlayerRole::Host, 21, supports_v5),
            controller_player(1, 2, PlayerRole::Guest, 34, supports_v5),
        ],
    }
}

fn controller_player(
    player_index: u8,
    display_number: u8,
    role: PlayerRole,
    last_seen_age_ms: u128,
    supports_v5: bool,
) -> PlayerSlotView {
    PlayerSlotView {
        player_index,
        display_number,
        role,
        status: PlayerStatus::Playing,
        runtime_state: PlayerRuntimeState::Playing,
        occupied: true,
        control_connected: true,
        input_connected: true,
        supports_state_file_relay: false,
        supports_rom_file_relay: false,
        supports_scheduled_start: supports_v5,
        supports_clock_sync: supports_v5,
        supports_fast_input_relay: supports_v5,
        last_seen_age_ms: Some(last_seen_age_ms),
        reconnect_grace_remaining_ms: None,
    }
}

fn scheduled_pause() -> SessionPauseView {
    SessionPauseView {
        sequence: 4,
        state: SessionPauseState::Pausing,
        reason: SessionPauseReason::Menu,
        requested_by_player_index: PlayerIndex::ONE,
        pause_at_frame: 126,
        paused_at_frame: None,
        acknowledged_player_indexes: Vec::new(),
        holders: vec![SessionPauseHolder {
            player_index: PlayerIndex::ONE,
            reason: SessionPauseReason::Menu,
        }],
    }
}

fn acknowledged_pause() -> SessionPauseView {
    SessionPauseView {
        sequence: 4,
        state: SessionPauseState::Paused,
        reason: SessionPauseReason::Menu,
        requested_by_player_index: PlayerIndex::ONE,
        pause_at_frame: 126,
        paused_at_frame: Some(126),
        acknowledged_player_indexes: vec![PlayerIndex::ONE, PlayerIndex::TWO],
        holders: vec![SessionPauseHolder {
            player_index: PlayerIndex::ONE,
            reason: SessionPauseReason::Menu,
        }],
    }
}

fn expand_room_fixture(message_fixture: &Value, fixture: &Value) -> Value {
    let mut expected = message_fixture.clone();
    if expected
        .as_object()
        .is_some_and(|object| object.get("room") == Some(&json!("$roomView")))
    {
        expected["room"] = fixture["roomView"].clone();
    }
    expected
}

fn assert_json_golden(actual: &Value, expected: &Value, path: &str) {
    if expected.as_str() == Some("$anyString") {
        assert!(
            actual.is_string(),
            "{path}: expected any JSON string, got {actual}"
        );
        return;
    }

    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            let actual_keys = actual.keys().collect::<BTreeSet<_>>();
            let expected_keys = expected.keys().collect::<BTreeSet<_>>();
            assert_eq!(
                actual_keys, expected_keys,
                "{path}: exact serialized object keys changed"
            );
            for (key, expected_value) in expected {
                assert_json_golden(
                    actual.get(key).expect("key checked above"),
                    expected_value,
                    &format!("{path}.{key}"),
                );
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(
                actual.len(),
                expected.len(),
                "{path}: serialized array length changed"
            );
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_json_golden(actual, expected, &format!("{path}[{index}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}: serialized value changed"),
    }
}
