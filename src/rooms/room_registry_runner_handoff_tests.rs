//! Focused tests for desktop-to-runner room capability handoff.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::NetplaySessionDescriptor;
use crate::rooms::{
    ClientTransportCapabilities, Clock, ConnectionId, InviteCode, InviteCodeGenerator,
    LinkCableDataPlaneError, PlayerIndex, PlayerStatus, RoomError, RoomRecoveryConfig, RoomStatus,
    UuidResumeTokenGenerator,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct StaticInviteCodeGenerator;

impl InviteCodeGenerator for StaticInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        InviteCode::parse("AB23-CD").expect("invite")
    }
}

struct ManualClock {
    now: Mutex<Instant>,
}

impl ManualClock {
    fn new(now: Instant) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    fn advance(&self, duration: Duration) {
        let mut now = self.now.lock().expect("clock lock");
        *now += duration;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.now.lock().expect("clock lock")
    }
}

#[tokio::test]
async fn runner_claim_replaces_provisional_socket_rotates_token_and_rejects_late_close() {
    let clock = Arc::new(ManualClock::new(Instant::now()));
    let registry = registry(clock);
    let (invite, provisional_connection, initial_join) = connect_host(&registry).await;
    let initial_epoch = initial_join.room.room_epoch;
    let initial_token = initial_join.resume_token.clone();

    registry
        .arm_runner_handoff(invite.clone(), provisional_connection)
        .await
        .expect("arm handoff");
    let runner_connection = ConnectionId::new();
    let resumed = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::ONE,
            initial_epoch,
            initial_token.clone(),
            runner_connection,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("runner claim");

    assert_ne!(resumed.resume_token, initial_token);
    assert_eq!(resumed.room.room_epoch, initial_epoch);
    assert!(matches!(
        registry
            .disconnect(invite.clone(), provisional_connection)
            .await,
        Err(RoomError::UnknownConnection)
    ));
    assert!(
        registry
            .room_view(invite.clone())
            .await
            .expect("room survives late close")
            .players[0]
            .control_connected
    );

    let replay = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::ONE,
            initial_epoch,
            initial_token,
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await;
    assert!(matches!(replay, Err(RoomError::ResumeTokenInvalid)));

    let input_connection = ConnectionId::new();
    registry
        .connect_input_socket(
            invite.clone(),
            PlayerIndex::ONE,
            resumed.room.room_epoch,
            resumed.room.session_epoch,
            resumed
                .input_socket_token
                .clone()
                .expect("controller input token"),
            input_connection,
        )
        .await
        .expect("fresh input grant");
    let input_replay = registry
        .connect_input_socket(
            invite,
            PlayerIndex::ONE,
            resumed.room.room_epoch,
            resumed.room.session_epoch,
            resumed.input_socket_token.expect("controller input token"),
            ConnectionId::new(),
        )
        .await;
    assert!(matches!(input_replay, Err(RoomError::ResumeTokenInvalid)));
}

#[tokio::test]
async fn link_runner_claim_atomically_replaces_live_provisional_endpoint() {
    let clock = Arc::new(ManualClock::new(Instant::now()));
    let registry = registry(clock);
    let created = registry
        .create_room(license("host"), ConnectionId::new(), link_descriptor())
        .await
        .expect("link room");
    let invite = InviteCode::parse(created.invite_code).expect("invite");
    let provisional_connection = ConnectionId::new();
    let initial_join = registry
        .connect_host(
            invite.clone(),
            license("host"),
            provisional_connection,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("provisional host");
    assert!(initial_join.input_socket_token.is_none());
    let mut provisional_receiver = registry
        .claim_link_cable_data_plane(invite.clone(), provisional_connection)
        .await
        .expect("provisional private route")
        .expect("link room")
        .receiver;

    registry
        .arm_runner_handoff(invite.clone(), provisional_connection)
        .await
        .expect("arm link handoff");
    let initial_token = initial_join.resume_token;
    let runner_connection = ConnectionId::new();
    let resumed = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::ONE,
            initial_join.room.room_epoch,
            initial_token.clone(),
            runner_connection,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("runner claims before provisional close");

    assert_ne!(resumed.resume_token, initial_token);
    assert!(resumed.input_socket_token.is_none());
    let runner_attachment = registry
        .claim_link_cable_data_plane(invite.clone(), runner_connection)
        .await
        .expect("runner private route")
        .expect("link room");
    assert_eq!(runner_attachment.snapshot.local_slot, PlayerIndex::ONE);
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(100), provisional_receiver.recv())
            .await
            .expect("provisional receiver invalidated"),
        Err(LinkCableDataPlaneError::AttachmentReplaced)
    );

    assert!(matches!(
        registry
            .disconnect(invite.clone(), provisional_connection)
            .await,
        Err(RoomError::UnknownConnection)
    ));
    let room = registry
        .room_view(invite)
        .await
        .expect("runner-owned room survives late close");
    assert!(room.players[0].control_connected);
}

#[tokio::test]
async fn active_takeover_requires_handoff_and_succeeds_just_before_deadline() {
    let clock = Arc::new(ManualClock::new(Instant::now()));
    let registry = registry(clock.clone());
    let (invite, provisional_connection, initial_join) = connect_host(&registry).await;

    let ordinary_takeover = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::ONE,
            initial_join.room.room_epoch,
            initial_join.resume_token.clone(),
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await;
    assert!(matches!(
        ordinary_takeover,
        Err(RoomError::ResumeTokenInvalid)
    ));

    registry
        .arm_runner_handoff(invite.clone(), provisional_connection)
        .await
        .expect("arm handoff");
    clock.advance(Duration::from_secs(60) - Duration::from_nanos(1));

    registry
        .reconnect_player(
            invite,
            PlayerIndex::ONE,
            initial_join.room.room_epoch,
            initial_join.resume_token,
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("claim immediately before deadline");
}

#[tokio::test]
async fn handoff_deadline_is_exact_and_expired_host_is_removed_by_sweep() {
    let clock = Arc::new(ManualClock::new(Instant::now()));
    let registry = registry(clock.clone());
    let (invite, provisional_connection, initial_join) = connect_host(&registry).await;

    registry
        .arm_runner_handoff(invite.clone(), provisional_connection)
        .await
        .expect("arm handoff");
    let disconnected = registry
        .disconnect(invite.clone(), provisional_connection)
        .await
        .expect("handoff disconnect");
    assert_eq!(disconnected.status, RoomStatus::WaitingForGuest);
    assert!(disconnected.players[0].occupied);
    assert!(!disconnected.players[0].control_connected);
    assert_eq!(disconnected.players[0].status, PlayerStatus::Reconnecting);

    clock.advance(Duration::from_secs(60));
    let at_deadline = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::ONE,
            initial_join.room.room_epoch,
            initial_join.resume_token,
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await;
    assert!(matches!(at_deadline, Err(RoomError::RecoveryExpired)));

    let removed = registry
        .remove_expired_waiting_rooms(clock.now(), Duration::from_secs(600))
        .await;
    assert_eq!(removed, 1);
    assert!(matches!(
        registry.room_view(invite).await,
        Err(RoomError::NotFound)
    ));
}

#[tokio::test]
async fn expired_guest_handoff_clears_only_guest_slot() {
    let clock = Arc::new(ManualClock::new(Instant::now()));
    let registry = registry(clock.clone());
    let (invite, _host_connection, _host_join) = connect_host(&registry).await;
    let guest_connection = ConnectionId::new();
    registry
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_connection,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest");
    registry
        .arm_runner_handoff(invite.clone(), guest_connection)
        .await
        .expect("arm guest handoff");
    registry
        .disconnect(invite.clone(), guest_connection)
        .await
        .expect("guest handoff disconnect");

    clock.advance(Duration::from_secs(60));
    let removed = registry
        .remove_expired_waiting_rooms(clock.now(), Duration::from_secs(600))
        .await;
    let room = registry.room_view(invite).await.expect("host room remains");

    assert_eq!(removed, 0);
    assert_eq!(room.status, RoomStatus::WaitingForGuest);
    assert!(room.players[0].occupied);
    assert!(room.players[0].control_connected);
    assert!(!room.players[1].occupied);
}

#[tokio::test]
async fn both_runner_claims_keep_epochs_stable_and_input_grants_usable() {
    let clock = Arc::new(ManualClock::new(Instant::now()));
    let registry = registry(clock);
    let (invite, host_provisional, host_join) = connect_host(&registry).await;
    registry
        .arm_runner_handoff(invite.clone(), host_provisional)
        .await
        .expect("arm host");

    let guest_provisional = ConnectionId::new();
    let guest_join = registry
        .connect_guest(
            invite.clone(),
            license("guest"),
            guest_provisional,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest");
    registry
        .arm_runner_handoff(invite.clone(), guest_provisional)
        .await
        .expect("arm guest");

    let host_runner = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::ONE,
            host_join.room.room_epoch,
            host_join.resume_token,
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("host runner");
    let guest_runner = registry
        .reconnect_player(
            invite.clone(),
            PlayerIndex::TWO,
            guest_join.room.room_epoch,
            guest_join.resume_token,
            ConnectionId::new(),
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("guest runner");

    assert_eq!(host_runner.room.room_epoch, guest_runner.room.room_epoch);
    assert_eq!(
        host_runner.room.session_epoch,
        guest_runner.room.session_epoch
    );

    registry
        .connect_input_socket(
            invite.clone(),
            PlayerIndex::ONE,
            guest_runner.room.room_epoch,
            guest_runner.room.session_epoch,
            host_runner
                .input_socket_token
                .expect("controller input token"),
            ConnectionId::new(),
        )
        .await
        .expect("host input");
    registry
        .connect_input_socket(
            invite,
            PlayerIndex::TWO,
            guest_runner.room.room_epoch,
            guest_runner.room.session_epoch,
            guest_runner
                .input_socket_token
                .expect("controller input token"),
            ConnectionId::new(),
        )
        .await
        .expect("guest input");
}

#[tokio::test]
async fn cancelled_handoff_restores_ordinary_send_failure_cleanup() {
    let clock = Arc::new(ManualClock::new(Instant::now()));
    let registry = registry(clock);
    let (invite, provisional_connection, _join) = connect_host(&registry).await;

    registry
        .arm_runner_handoff(invite.clone(), provisional_connection)
        .await
        .expect("arm handoff");
    registry
        .cancel_runner_handoff(invite.clone(), provisional_connection)
        .await
        .expect("rollback failed delivery");
    registry
        .disconnect(invite.clone(), provisional_connection)
        .await
        .expect("ordinary disconnect");

    assert!(matches!(
        registry.room_view(invite).await,
        Err(RoomError::NotFound)
    ));
}

fn registry(clock: Arc<ManualClock>) -> InMemoryRoomRegistry {
    InMemoryRoomRegistry::with_dependencies(
        Arc::new(StaticInviteCodeGenerator),
        Arc::new(UuidResumeTokenGenerator),
        clock,
        RoomRecoveryConfig::default(),
    )
}

async fn connect_host(
    registry: &InMemoryRoomRegistry,
) -> (InviteCode, ConnectionId, crate::rooms::RoomJoin) {
    let room = registry
        .create_room(license("host"), ConnectionId::new(), descriptor())
        .await
        .expect("room");
    let invite = InviteCode::parse(room.invite_code).expect("invite");
    let connection_id = ConnectionId::new();
    let join = registry
        .connect_host(
            invite.clone(),
            license("host"),
            connection_id,
            ClientTransportCapabilities::default(),
        )
        .await
        .expect("host");

    (invite, connection_id, join)
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
            "maxPlayers": 2,
            "transport": "relay"
        }
    }))
    .expect("link descriptor")
}
