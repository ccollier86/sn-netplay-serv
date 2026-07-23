//! Focused tests for server-authoritative room-scope allocation and privacy.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::{
    LEGACY_NETPLAY_PROTOCOL_VERSION, NETPLAY_PROTOCOL_VERSION, NetplaySessionDescriptor,
};
use crate::rooms::{
    ClientTransportCapabilities, ConnectionId, InviteCode, InviteCodeGenerator, NetplayRoom,
    NoopRoomDebugEventSink, PlayerIndex, RoomRecoveryConfig, RoomScope, RoomScopeAllocator,
    SystemClock, UuidResumeTokenGenerator,
};
use crate::voice::DisabledVoiceBroker;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct SequenceInviteCodeGenerator {
    values: Mutex<VecDeque<InviteCode>>,
}

impl SequenceInviteCodeGenerator {
    fn new(values: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            values: Mutex::new(
                values
                    .into_iter()
                    .map(|value| InviteCode::parse(value).expect("valid invite"))
                    .collect(),
            ),
        }
    }
}

impl InviteCodeGenerator for SequenceInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        self.values
            .lock()
            .expect("invite sequence lock")
            .pop_front()
            .expect("invite sequence exhausted")
    }
}

struct SequenceRoomScopeAllocator {
    values: Mutex<VecDeque<RoomScope>>,
}

impl SequenceRoomScopeAllocator {
    fn new(values: impl IntoIterator<Item = u64>) -> Self {
        Self {
            values: Mutex::new(
                values
                    .into_iter()
                    .map(|value| RoomScope::new(value).expect("valid room scope"))
                    .collect(),
            ),
        }
    }
}

impl RoomScopeAllocator for SequenceRoomScopeAllocator {
    fn allocate(&self) -> RoomScope {
        self.values
            .lock()
            .expect("room-scope sequence lock")
            .pop_front()
            .expect("room-scope sequence exhausted")
    }
}

#[test]
fn link_provider_resets_and_epoch_changes_preserve_room_identity_scope() {
    const LINK_SCOPE: u64 = 8_765_432_099;

    let mut room = NetplayRoom::new_with_protocol_resume_and_scope(
        license("link-host"),
        ConnectionId::new(),
        InviteCode::parse("LK23AB").expect("link invite"),
        link_descriptor(),
        LEGACY_NETPLAY_PROTOCOL_VERSION,
        String::new(),
        String::new(),
        std::time::Instant::now(),
        RoomScope::new(LINK_SCOPE).expect("link scope"),
    );
    let room_id = room.room_id();

    room.reset_sync_state();
    room.bump_room_epoch();
    room.bump_session_epoch();

    assert_eq!(room.room_id(), room_id);
    assert_eq!(room.link_room_scope().get(), LINK_SCOPE);
}

#[tokio::test]
async fn scope_is_unique_stable_and_absent_from_controller_surfaces() {
    const FIRST_SCOPE: u64 = 8_765_432_101;
    const SECOND_SCOPE: u64 = 8_765_432_102;

    let registry = InMemoryRoomRegistry::with_dependencies_event_sink_voice_and_room_scope(
        Arc::new(SequenceInviteCodeGenerator::new(["AB23CD", "EF45GH"])),
        Arc::new(UuidResumeTokenGenerator),
        Arc::new(SystemClock),
        RoomRecoveryConfig::default(),
        Arc::new(NoopRoomDebugEventSink),
        Arc::new(DisabledVoiceBroker),
        Arc::new(SequenceRoomScopeAllocator::new([FIRST_SCOPE, SECOND_SCOPE])),
    );

    let provisional_connection = ConnectionId::new();
    let first_view = registry
        .create_room_with_protocol(
            license("host-one"),
            ConnectionId::new(),
            descriptor(),
            LEGACY_NETPLAY_PROTOCOL_VERSION,
        )
        .await
        .expect("v4 room");
    let first_invite = InviteCode::parse(&first_view.invite_code).expect("first invite");
    let first_room_id = first_view.room_id;
    assert_eq!(
        room_scope(&registry, &first_invite).await.get(),
        FIRST_SCOPE
    );
    assert_private_controller_json(&first_view, FIRST_SCOPE);

    let initial_join = registry
        .connect_host(
            first_invite.clone(),
            license("host-one"),
            provisional_connection,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("provisional host join");
    registry
        .arm_runner_handoff(first_invite.clone(), provisional_connection)
        .await
        .expect("arm reconnect handoff");
    registry
        .reconnect_player(
            first_invite.clone(),
            PlayerIndex::ONE,
            initial_join.room.room_epoch,
            initial_join.resume_token,
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("host reconnect");
    assert_eq!(
        room_scope(&registry, &first_invite).await.get(),
        FIRST_SCOPE
    );

    let epoch_before_guest = registry
        .room_view(first_invite.clone())
        .await
        .expect("room before provider reset")
        .session_epoch;
    registry
        .connect_guest(
            first_invite.clone(),
            license("guest-one"),
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest joins and resets provider state");
    let after_guest = registry
        .room_view(first_invite.clone())
        .await
        .expect("room after provider reset");
    assert!(after_guest.session_epoch > epoch_before_guest);
    assert_eq!(
        room_scope(&registry, &first_invite).await.get(),
        FIRST_SCOPE
    );
    assert_private_controller_json(&after_guest, FIRST_SCOPE);

    let room_debug = {
        let rooms = registry.invite_codes.read().await;
        format!(
            "{:?}",
            rooms
                .get(first_invite.normalized())
                .expect("stored first room")
                .room
        )
    };
    assert!(!room_debug.contains(&FIRST_SCOPE.to_string()));
    let debug_events = registry
        .room_events(first_invite.clone(), 20)
        .await
        .expect("debug events");
    let debug_json = serde_json::to_string(&debug_events).expect("serialize debug events");
    assert_scope_absent(&debug_json, FIRST_SCOPE);

    let second_view = registry
        .create_room_with_protocol(
            license("host-two"),
            ConnectionId::new(),
            descriptor(),
            NETPLAY_PROTOCOL_VERSION,
        )
        .await
        .expect("v5 room");
    let second_invite = InviteCode::parse(&second_view.invite_code).expect("second invite");
    let second_scope = room_scope(&registry, &second_invite).await;

    assert_ne!(second_view.room_id, first_room_id);
    assert_eq!(second_scope.get(), SECOND_SCOPE);
    assert_ne!(second_scope, room_scope(&registry, &first_invite).await);
    assert_private_controller_json(&second_view, SECOND_SCOPE);
}

async fn room_scope(registry: &InMemoryRoomRegistry, invite: &InviteCode) -> RoomScope {
    registry
        .invite_codes
        .read()
        .await
        .get(invite.normalized())
        .expect("stored room")
        .room
        .link_room_scope()
}

fn assert_private_controller_json(room: &crate::rooms::RoomView, scope: u64) {
    let json = serde_json::to_string(room).expect("serialize room view");
    assert_scope_absent(&json, scope);
}

fn assert_scope_absent(serialized: &str, scope: u64) {
    assert!(!serialized.contains("roomScope"));
    assert!(!serialized.contains("room_scope"));
    assert!(!serialized.contains(&scope.to_string()));
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
            "romSha256": "b".repeat(64),
            "contentKey": "gba-pokemon-ruby"
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
