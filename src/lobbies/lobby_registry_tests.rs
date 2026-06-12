//! Tests for persistent lobby registry behavior.

use crate::auth::{ClientKind, VerifiedLicense};
use crate::lobbies::{
    CreateLobbyParams, InMemoryLobbyRegistry, JoinLobbyParams, LobbyActivityKind,
    LobbyClientCapabilities, LobbyError, LobbyEvent, LobbyGameCandidate, LobbyGameReadinessStatus,
    LobbyPlayerRole, LobbyPlayerStatus, LobbyRegistry, LobbyServerCapabilities, LobbyStatus,
    LobbyVisibility, MAX_LOBBY_PLAYERS, PublicLobbyEventReceiver,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, ResumeToken, ResumeTokenGenerator,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn create_lobby_reserves_host_as_player_one() {
    let registry = registry();

    let join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");

    assert_eq!(join.player_index.zero_based(), 0);
    assert!(!join.resume_token.is_empty());
    assert_eq!(join.lobby.players[0].display_number, 1);
    assert_eq!(join.lobby.players[0].role, LobbyPlayerRole::Host);
    assert_eq!(join.lobby.players[0].color, "cyan");
}

#[tokio::test]
async fn join_lobby_assigns_players_by_join_order() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");

    let player_two = registry
        .join_lobby(invite.clone(), license("guest-1"), join_params())
        .await
        .expect("joined");
    let player_three = registry
        .join_lobby(invite, license("guest-2"), join_params())
        .await
        .expect("joined");

    assert_eq!(player_two.player_index.zero_based(), 1);
    assert_eq!(player_two.lobby.players[1].color, "violet");
    assert_eq!(player_three.player_index.zero_based(), 2);
    assert_eq!(player_three.lobby.players[2].color, "amber");
}

#[tokio::test]
async fn public_lobbies_only_include_public_joinable_lobbies() {
    let registry = registry();

    registry
        .create_lobby(license("private-host"), create_params())
        .await
        .expect("private created");
    assert!(registry.public_lobbies().await.is_empty());

    let host_join = registry
        .create_lobby(license("public-host"), create_public_params())
        .await
        .expect("public created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let public_lobbies = registry.public_lobbies().await;

    assert_eq!(public_lobbies.len(), 1);
    assert_eq!(public_lobbies[0].visibility, LobbyVisibility::Public);
    assert_eq!(public_lobbies[0].hosted_by, "Host");
    assert_eq!(public_lobbies[0].player_count, 1);
    assert_eq!(public_lobbies[0].open_slots, 1);
    assert_eq!(
        public_lobbies[0]
            .selected_game
            .as_ref()
            .expect("selected")
            .title,
        "Starlight Ruins"
    );

    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_params(),
            guest_connection,
        )
        .await
        .expect("guest connected");
    assert!(registry.public_lobbies().await.is_empty());

    registry
        .leave_lobby(invite, guest_connection)
        .await
        .expect("guest left");
    assert_eq!(registry.public_lobbies().await.len(), 1);
}

#[tokio::test]
async fn public_lobby_directory_emits_when_joinable_set_changes() {
    let registry = registry();
    let mut public_events = registry.subscribe_public_lobbies().await;

    let host_join = registry
        .create_lobby(license("public-host"), create_public_params())
        .await
        .expect("public created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    expect_public_lobby_event(&mut public_events).await;

    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_params(),
            guest_connection,
        )
        .await
        .expect("guest connected");
    expect_public_lobby_event(&mut public_events).await;

    registry
        .leave_lobby(invite, guest_connection)
        .await
        .expect("guest left");
    expect_public_lobby_event(&mut public_events).await;
}

#[tokio::test]
async fn joining_again_refreshes_existing_player_slot() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");

    let first = registry
        .join_lobby(invite.clone(), license("guest"), join_params())
        .await
        .expect("joined");
    let refreshed = registry
        .join_lobby(invite, license("guest"), join_params())
        .await
        .expect("refreshed");

    assert_eq!(first.player_index, refreshed.player_index);
    assert_eq!(refreshed.lobby.players[1].display_number, 2);
    assert!(!refreshed.lobby.players[2].occupied);
}

#[tokio::test]
async fn reconnect_reclaims_slot_with_prior_epoch_and_matching_identity() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let first_connection = ConnectionId::new();
    let joined = registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_params(),
            first_connection,
        )
        .await
        .expect("joined");
    let observed_epoch_before_disconnect = joined.lobby.lobby_epoch;

    registry
        .disconnect_lobby(invite.clone(), first_connection)
        .await
        .expect("disconnected");

    let reconnected = registry
        .reconnect_lobby_player(
            invite,
            license("guest"),
            join_params(),
            joined.player_index,
            observed_epoch_before_disconnect,
            joined.resume_token,
            ConnectionId::new(),
        )
        .await
        .expect("reconnected");

    assert_eq!(reconnected.player_index.zero_based(), 1);
    assert!(reconnected.lobby.players[1].connected);
    assert_eq!(
        reconnected.lobby.players[1].status,
        LobbyPlayerStatus::Connected
    );
}

#[tokio::test]
async fn reconnect_rejects_valid_token_from_different_identity() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let joined = registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_params(),
            ConnectionId::new(),
        )
        .await
        .expect("joined");

    let error = registry
        .reconnect_lobby_player(
            invite,
            license("attacker"),
            join_params(),
            joined.player_index,
            joined.lobby.lobby_epoch,
            joined.resume_token,
            ConnectionId::new(),
        )
        .await
        .expect_err("identity mismatch");

    assert!(matches!(error, LobbyError::PlayerSlotUnavailable));
}

#[tokio::test]
async fn fifth_player_is_rejected() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");

    for index in 1..=3 {
        registry
            .join_lobby(
                invite.clone(),
                license(&format!("guest-{index}")),
                join_params(),
            )
            .await
            .expect("joined");
    }

    let error = registry
        .join_lobby(invite, license("guest-4"), join_params())
        .await
        .expect_err("full");

    assert!(matches!(error, LobbyError::LobbyFull));
}

#[tokio::test]
async fn intentional_guest_leave_frees_the_lobby_slot() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_params(),
            guest_connection,
        )
        .await
        .expect("joined");

    let lobby = registry
        .leave_lobby(invite, guest_connection)
        .await
        .expect("left");

    assert_eq!(lobby.status, LobbyStatus::Open);
    assert!(!lobby.players[1].occupied);
    assert_eq!(lobby.players[1].status, LobbyPlayerStatus::Empty);
}

#[tokio::test]
async fn intentional_host_leave_closes_the_lobby() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_params(),
            host_connection,
        )
        .await
        .expect("host connected");

    let lobby = registry
        .leave_lobby(invite, host_connection)
        .await
        .expect("closed");

    assert_eq!(lobby.status, LobbyStatus::Closed);
    assert_eq!(lobby.players[0].status, LobbyPlayerStatus::Disconnected);
    assert!(!lobby.players[0].connected);
}

#[tokio::test]
async fn lobby_view_reports_server_capabilities() {
    let registry = InMemoryLobbyRegistry::with_generators_and_capabilities(
        Arc::new(SequenceInviteCodeGenerator::default()),
        Arc::new(SequenceResumeTokenGenerator::default()),
        LobbyServerCapabilities::current(MAX_LOBBY_PLAYERS, true, true),
    );

    let join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");

    assert!(join.lobby.capabilities.supports_temporary_session_rom_relay);
    assert!(join.lobby.capabilities.supports_lobby_voice);
    assert_eq!(join.lobby.capabilities.max_players, 4);
}

#[tokio::test]
async fn connected_lobby_socket_broadcasts_state_changes() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");

    let connection_id = ConnectionId::new();
    let joined = registry
        .connect_lobby(invite, license("host"), join_params(), connection_id)
        .await
        .expect("connected");
    let event = recv_lobby_event(&mut events).await;

    assert_eq!(joined.player_index.zero_based(), 0);
    assert!(matches!(event, LobbyEvent::LobbyStateChanged(_)));
}

#[tokio::test]
async fn host_can_select_game_and_broadcast_lobby_state() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");
    let connection_id = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_params(),
            connection_id,
        )
        .await
        .expect("connected");
    let _ = recv_lobby_event(&mut events).await;

    let view = registry
        .select_lobby_game(invite, connection_id, game_candidate())
        .await
        .expect("selected");
    let event = recv_lobby_event(&mut events).await;

    assert_eq!(view.status, crate::lobbies::LobbyStatus::GameSelected);
    assert_eq!(
        view.selected_game.expect("selected").game.title,
        "Starlight Ruins"
    );
    assert!(matches!(event, LobbyEvent::LobbyStateChanged(_)));
}

#[tokio::test]
async fn players_report_readiness_for_selected_game() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_params(),
            host_connection,
        )
        .await
        .expect("host connected");

    let selected = registry
        .select_lobby_game(invite.clone(), host_connection, game_candidate())
        .await
        .expect("selected")
        .selected_game
        .expect("selected game");

    let view = registry
        .set_lobby_game_readiness(
            invite,
            host_connection,
            selected.proposal_id,
            LobbyGameReadinessStatus::Ready,
            Some("  exact ROM matched  ".to_string()),
        )
        .await
        .expect("ready");

    assert_eq!(view.game_readiness.len(), 1);
    assert_eq!(
        view.game_readiness[0].status,
        LobbyGameReadinessStatus::Ready
    );
    assert_eq!(
        view.game_readiness[0].detail.as_deref(),
        Some("exact ROM matched")
    );
}

#[tokio::test]
async fn host_launch_requires_connected_players_ready() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_params(),
            host_connection,
        )
        .await
        .expect("host connected");
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_params(),
            guest_connection,
        )
        .await
        .expect("guest connected");
    let selected = registry
        .select_lobby_game(invite.clone(), host_connection, game_candidate())
        .await
        .expect("selected")
        .selected_game
        .expect("selected game");

    let not_ready = registry
        .request_lobby_game_launch(invite.clone(), host_connection, selected.proposal_id)
        .await
        .expect_err("guest not ready");
    assert!(matches!(not_ready, LobbyError::PlayersNotReady));

    for connection in [host_connection, guest_connection] {
        registry
            .set_lobby_game_readiness(
                invite.clone(),
                connection,
                selected.proposal_id,
                LobbyGameReadinessStatus::Ready,
                None,
            )
            .await
            .expect("ready");
    }

    let launched = registry
        .request_lobby_game_launch(invite.clone(), host_connection, selected.proposal_id)
        .await
        .expect("launched");

    assert_eq!(launched.status, LobbyStatus::InGame);
    assert_eq!(
        launched
            .pending_launch
            .as_ref()
            .expect("launch")
            .proposal_id,
        selected.proposal_id
    );
    assert_eq!(
        launched.pending_launch.expect("launch").status,
        crate::lobbies::LobbyGameLaunchStatus::Preparing
    );

    let published = registry
        .publish_lobby_game_room(
            invite.clone(),
            host_connection,
            selected.proposal_id,
            InviteCode::parse("AB23-CD").expect("room invite"),
        )
        .await
        .expect("published");

    let pending_launch = published.pending_launch.expect("launch");

    assert_eq!(
        pending_launch.status,
        crate::lobbies::LobbyGameLaunchStatus::Ready
    );
    assert_eq!(pending_launch.room_invite_code.as_deref(), Some("AB23-CD"));

    let returned = registry
        .return_lobby_from_game(invite, guest_connection, selected.proposal_id)
        .await
        .expect("returned");

    assert_eq!(returned.status, LobbyStatus::GameSelected);
    assert!(returned.pending_launch.is_none());
    assert!(returned.game_readiness.is_empty());
}

#[tokio::test]
async fn return_to_lobby_requires_active_launch() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_params(),
            host_connection,
        )
        .await
        .expect("host connected");
    let selected = registry
        .select_lobby_game(invite.clone(), host_connection, game_candidate())
        .await
        .expect("selected")
        .selected_game
        .expect("selected game");

    let error = registry
        .return_lobby_from_game(invite, host_connection, selected.proposal_id)
        .await
        .expect_err("cannot return before a child launch exists");

    assert!(matches!(error, LobbyError::StaleGameProposal));
}

#[tokio::test]
async fn lobby_chat_is_sanitized_and_broadcast() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");
    let connection_id = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_params(),
            connection_id,
        )
        .await
        .expect("connected");
    let _ = recv_lobby_event(&mut events).await;

    let chat = registry
        .send_lobby_chat(invite, connection_id, "  hello\n\nworld  ".to_string())
        .await
        .expect("chat");
    let event = recv_lobby_event(&mut events).await;

    assert_eq!(chat.body, "hello world");
    assert!(matches!(event, LobbyEvent::ChatMessage(_)));
}

#[tokio::test]
async fn idle_lobby_expiration_closes_and_removes_lobby() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");

    let expired = registry.expire_idle_lobbies(Duration::ZERO).await;
    let event = recv_lobby_event(&mut events).await;
    let error = registry
        .lobby_view(invite)
        .await
        .expect_err("expired lobby is removed");

    assert_eq!(expired, 1);
    assert!(matches!(error, LobbyError::NotFound));
    match event {
        LobbyEvent::LobbyClosed { lobby, reason } => {
            assert_eq!(reason, "inactive");
            assert_eq!(lobby.status, LobbyStatus::Closed);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn recorded_lobby_activity_keeps_lobby_within_idle_window() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_params(),
            host_connection,
        )
        .await
        .expect("connected");

    registry
        .record_lobby_activity(
            invite.clone(),
            host_connection,
            LobbyActivityKind::GameplayActive,
        )
        .await
        .expect("activity recorded");

    let expired = registry
        .expire_idle_lobbies(Duration::from_secs(3600))
        .await;
    let lobby = registry.lobby_view(invite).await.expect("retained");

    assert_eq!(expired, 0);
    assert_eq!(lobby.status, LobbyStatus::Open);
}

async fn recv_lobby_event(events: &mut crate::lobbies::LobbyEventReceiver) -> LobbyEvent {
    timeout(Duration::from_millis(250), events.recv())
        .await
        .expect("event timeout")
        .expect("event")
}

fn game_candidate() -> LobbyGameCandidate {
    LobbyGameCandidate {
        title: "Starlight Ruins".to_string(),
        system_id: "snes".to_string(),
        core_id: "snes9x".to_string(),
        content_sha256: Some("c".repeat(64)),
        rom_size_bytes: Some(2_097_152),
        start_state_label: Some("fresh".to_string()),
    }
}

fn registry() -> InMemoryLobbyRegistry {
    InMemoryLobbyRegistry::with_generators(
        Arc::new(SequenceInviteCodeGenerator::default()),
        Arc::new(SequenceResumeTokenGenerator::default()),
    )
}

async fn expect_public_lobby_event(receiver: &mut PublicLobbyEventReceiver) {
    timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("public lobby event timed out")
        .expect("public lobby event channel open");
}

fn create_params() -> CreateLobbyParams {
    CreateLobbyParams {
        display_name: Some("Host".to_string()),
        capabilities: LobbyClientCapabilities::desktop_default(),
        initial_game: None,
        voice: None,
        visibility: LobbyVisibility::Private,
    }
}

fn create_public_params() -> CreateLobbyParams {
    CreateLobbyParams {
        display_name: Some("Host".to_string()),
        capabilities: LobbyClientCapabilities::desktop_default(),
        initial_game: Some(game_candidate()),
        voice: None,
        visibility: LobbyVisibility::Public,
    }
}

fn join_params() -> JoinLobbyParams {
    JoinLobbyParams {
        display_name: None,
        capabilities: LobbyClientCapabilities::desktop_default(),
    }
}

fn license(subject: &str) -> VerifiedLicense {
    VerifiedLicense::with_entitlement(
        ClientKind::Desktop,
        format!("install-{subject}"),
        subject,
        "premium",
        vec!["netplay".to_string()],
        true,
        false,
    )
}

#[derive(Default)]
struct SequenceInviteCodeGenerator {
    next: AtomicUsize,
}

impl InviteCodeGenerator for SequenceInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        let code = match index {
            0 => "AB23-CD",
            1 => "EF45-GH",
            _ => "JK67-LM",
        };

        InviteCode::parse(code).expect("code")
    }
}

#[derive(Default)]
struct SequenceResumeTokenGenerator {
    next: AtomicUsize,
}

impl ResumeTokenGenerator for SequenceResumeTokenGenerator {
    fn generate(&self) -> ResumeToken {
        let index = self.next.fetch_add(1, Ordering::Relaxed);

        ResumeToken::new(format!("resume-token-{index}"))
    }
}
