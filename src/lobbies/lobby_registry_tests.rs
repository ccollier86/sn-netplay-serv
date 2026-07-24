//! Tests for persistent lobby registry behavior.

use crate::auth::{ClientKind, VerifiedLicense};
use crate::lobbies::{
    CreateLobbyParams, InMemoryLobbyRegistry, JoinLobbyParams, LobbyActivityKind,
    LobbyClientCapabilities, LobbyError, LobbyEvent, LobbyGameCandidate, LobbyGameLaunchStatus,
    LobbyGameReadinessStatus, LobbyLinkCableClientCapabilities, LobbyLinkCableLaunchState,
    LobbyLinkProtocolFamily, LobbyMultiplayerSessionKind, LobbyPlayerRemovalReason,
    LobbyPlayerRole, LobbyPlayerStatus, LobbyRegistry, LobbyReturnReason, LobbyServerCapabilities,
    LobbyStatus, LobbyView, LobbyVisibility, MAX_LOBBY_PLAYERS, PublicLobbyEventReceiver,
    ReconnectLobbyPlayerRequest,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex, ResumeToken, ResumeTokenGenerator,
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
        .reconnect_lobby_player(ReconnectLobbyPlayerRequest {
            invite_code: invite,
            player: license("guest"),
            params: join_params(),
            player_index: joined.player_index,
            lobby_epoch: observed_epoch_before_disconnect,
            resume_token: joined.resume_token,
            connection_id: ConnectionId::new(),
        })
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
        .reconnect_lobby_player(ReconnectLobbyPlayerRequest {
            invite_code: invite,
            player: license("attacker"),
            params: join_params(),
            player_index: joined.player_index,
            lobby_epoch: joined.lobby.lobby_epoch,
            resume_token: joined.resume_token,
            connection_id: ConnectionId::new(),
        })
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
async fn host_removal_erases_guest_membership_before_broadcasting_roster() {
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
    let guest_connection = ConnectionId::new();
    let guest_join = registry
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
        .expect("selected");
    let proposal_id = selected.selected_game.as_ref().expect("game").proposal_id;
    let ready = registry
        .set_lobby_game_readiness(
            invite.clone(),
            guest_connection,
            proposal_id,
            LobbyGameReadinessStatus::Ready,
            None,
        )
        .await
        .expect("guest ready");
    let observed_epoch = ready.lobby_epoch;
    let old_resume_token = guest_join.resume_token;
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");

    let removed = registry
        .remove_lobby_player(
            invite.clone(),
            host_connection,
            observed_epoch,
            guest_join.player_index,
        )
        .await
        .expect("removed");

    assert_eq!(removed.status, LobbyStatus::GameSelected);
    assert!(removed.selected_game.is_some());
    assert!(!removed.players[1].occupied);
    assert_eq!(removed.players[1].status, LobbyPlayerStatus::Empty);
    assert!(
        removed
            .game_readiness
            .iter()
            .all(|readiness| readiness.player_index != 1)
    );

    match recv_lobby_event(&mut events).await {
        LobbyEvent::PlayerRemoved {
            target,
            player_index,
            reason,
            lobby,
        } => {
            assert_eq!(target, guest_connection);
            assert_eq!(player_index, 1);
            assert_eq!(reason, LobbyPlayerRemovalReason::RemovedByHost);
            assert_eq!(lobby.event_seq, removed.event_seq);
            assert_eq!(lobby.lobby_epoch, removed.lobby_epoch);
            assert!(!lobby.players[1].occupied);
        }
        other => panic!("unexpected first removal event: {other:?}"),
    }
    match recv_lobby_event(&mut events).await {
        LobbyEvent::LobbyStateChanged(lobby) => {
            assert_eq!(lobby.event_seq, removed.event_seq);
            assert_eq!(lobby.lobby_epoch, removed.lobby_epoch);
        }
        other => panic!("unexpected roster event: {other:?}"),
    }

    let reconnect_error = registry
        .reconnect_lobby_player(ReconnectLobbyPlayerRequest {
            invite_code: invite.clone(),
            player: license("guest"),
            params: join_params(),
            player_index: guest_join.player_index,
            lobby_epoch: observed_epoch,
            resume_token: old_resume_token.clone(),
            connection_id: ConnectionId::new(),
        })
        .await
        .expect_err("removed resume token must not reclaim the slot");
    assert!(matches!(reconnect_error, LobbyError::PlayerSlotUnavailable));

    let rejoined = registry
        .connect_lobby(invite, license("guest"), join_params(), ConnectionId::new())
        .await
        .expect("removed player may join again as a new membership");
    assert_eq!(rejoined.player_index, PlayerIndex::TWO);
    assert_ne!(rejoined.resume_token, old_resume_token);
}

#[tokio::test]
async fn player_removal_rejects_non_host_stale_host_and_empty_targets() {
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
    let guest_connection = ConnectionId::new();
    let joined = registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_params(),
            guest_connection,
        )
        .await
        .expect("guest connected");
    let epoch = joined.lobby.lobby_epoch;

    let error = registry
        .remove_lobby_player(invite.clone(), guest_connection, epoch, PlayerIndex::TWO)
        .await
        .expect_err("guest cannot remove a player");
    assert!(matches!(error, LobbyError::PlayerRemovalHostOnly));

    let error = registry
        .remove_lobby_player(invite.clone(), host_connection, epoch, PlayerIndex::ONE)
        .await
        .expect_err("host cannot remove itself");
    assert!(matches!(error, LobbyError::CannotRemoveLobbyHost));

    let player_three = PlayerIndex::new(2, MAX_LOBBY_PLAYERS).expect("player three");
    let error = registry
        .remove_lobby_player(invite.clone(), host_connection, epoch, player_three)
        .await
        .expect_err("empty target cannot be removed");
    assert!(matches!(error, LobbyError::LobbyPlayerNotFound));

    let error = registry
        .remove_lobby_player(
            invite,
            host_connection,
            epoch.saturating_sub(1),
            PlayerIndex::TWO,
        )
        .await
        .expect_err("stale removal must fail");
    assert!(matches!(error, LobbyError::StaleLobbyEpoch));
}

#[tokio::test]
async fn player_removal_is_unavailable_after_launch_begins() {
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
    let selected = registry
        .select_lobby_game(invite.clone(), host_connection, game_candidate())
        .await
        .expect("selected");
    let proposal_id = selected.selected_game.expect("game").proposal_id;
    registry
        .set_lobby_game_readiness(
            invite.clone(),
            host_connection,
            proposal_id,
            LobbyGameReadinessStatus::Ready,
            None,
        )
        .await
        .expect("host ready");
    registry
        .set_lobby_game_readiness(
            invite.clone(),
            guest_connection,
            proposal_id,
            LobbyGameReadinessStatus::Ready,
            None,
        )
        .await
        .expect("guest ready");
    let launched = registry
        .request_lobby_game_launch(invite.clone(), host_connection, proposal_id)
        .await
        .expect("launched");

    let error = registry
        .remove_lobby_player(
            invite,
            host_connection,
            launched.lobby_epoch,
            PlayerIndex::TWO,
        )
        .await
        .expect_err("active launch blocks removal");

    assert!(matches!(error, LobbyError::LobbyPlayerRemovalUnavailable));
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
    assert!(join.lobby.capabilities.supports_lobby_player_removal);
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
async fn link_lobby_accepts_different_player_roms_and_independent_launch_order() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_link_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_link_params(),
            host_connection,
        )
        .await
        .expect("host connected");
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_link_params(),
            guest_connection,
        )
        .await
        .expect("guest connected");

    let host_view = registry
        .select_lobby_link_cable_game(
            invite.clone(),
            host_connection,
            link_game("Host GBA", "gba", 'a'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            Some(InviteCode::parse("EF45-GH").expect("room invite")),
        )
        .await
        .expect("host selected");
    let host_link = host_view
        .multiplayer_extension
        .as_ref()
        .and_then(|extension| extension.link_cable.as_ref())
        .expect("link extension");
    assert_eq!(host_view.capabilities.max_players, 2);
    assert_eq!(host_link.max_players, 2);
    assert_eq!(host_link.players[0].selection_generation, 1);
    assert_eq!(
        host_link.players[0]
            .selected_game
            .as_ref()
            .unwrap()
            .content_sha256,
        Some("a".repeat(64))
    );

    let guest_view = registry
        .select_lobby_link_cable_game(
            invite.clone(),
            guest_connection,
            link_game("Guest GBA", "gba", 'b'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            None,
        )
        .await
        .expect("guest selected");
    let extension = guest_view.multiplayer_extension.expect("extension");
    assert_eq!(
        extension.session_kind,
        LobbyMultiplayerSessionKind::LinkCable
    );
    let link = extension.link_cable.expect("link");
    assert_eq!(
        link.players[0].selected_game.as_ref().unwrap().title,
        "Host GBA"
    );
    assert_eq!(
        link.players[1].selected_game.as_ref().unwrap().title,
        "Guest GBA"
    );
    assert_ne!(
        link.players[0]
            .selected_game
            .as_ref()
            .unwrap()
            .content_sha256,
        link.players[1]
            .selected_game
            .as_ref()
            .unwrap()
            .content_sha256,
    );

    let guest_launch = registry
        .set_lobby_link_cable_launch_state(
            invite.clone(),
            guest_connection,
            link.players[1].selection_generation,
            LobbyLinkCableLaunchState::Launching,
            None,
        )
        .await
        .expect("guest launched first");
    let guest_link = guest_launch
        .multiplayer_extension
        .as_ref()
        .and_then(|extension| extension.link_cable.as_ref())
        .expect("link");
    assert_eq!(
        guest_link.players[1].launch_state,
        LobbyLinkCableLaunchState::Launching,
    );
    assert_eq!(
        guest_link.players[0].launch_state,
        LobbyLinkCableLaunchState::NotLaunched,
    );

    let host_launch = registry
        .set_lobby_link_cable_launch_state(
            invite,
            host_connection,
            guest_link.players[0].selection_generation,
            LobbyLinkCableLaunchState::Launching,
            Some(InviteCode::parse("EF45-GH").expect("room invite")),
        )
        .await
        .expect("host launched independently");
    let host_link = host_launch
        .multiplayer_extension
        .as_ref()
        .and_then(|extension| extension.link_cable.as_ref())
        .expect("link");
    assert_eq!(
        host_link.players[0].launch_state,
        LobbyLinkCableLaunchState::Launching,
    );
    assert_eq!(
        host_link.players[1].launch_state,
        LobbyLinkCableLaunchState::Launching,
    );
    assert!(host_launch.pending_launch.is_none());
    assert!(host_launch.game_readiness.is_empty());
}

#[tokio::test]
async fn authoritative_gba_v1_link_lobby_accepts_player_game_reselection() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_link_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_link_params(),
            host_connection,
        )
        .await
        .expect("host connected");
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_link_params(),
            guest_connection,
        )
        .await
        .expect("guest connected");

    registry
        .select_lobby_link_cable_game(
            invite.clone(),
            host_connection,
            link_game("Host GBA v1", "gba", 'a'),
            LobbyLinkProtocolFamily::GbaMultiV1,
            Some(InviteCode::parse("EF45-GH").expect("room invite")),
        )
        .await
        .expect("host established authoritative GBA v1 family");
    let first_guest_selection = registry
        .select_lobby_link_cable_game(
            invite.clone(),
            guest_connection,
            link_game("Guest GBA v1", "gba", 'b'),
            LobbyLinkProtocolFamily::GbaMultiV1,
            None,
        )
        .await
        .expect("guest selected in authoritative GBA v1 family");
    let first_generation = first_guest_selection
        .multiplayer_extension
        .as_ref()
        .and_then(|extension| extension.link_cable.as_ref())
        .expect("GBA v1 extension")
        .players[1]
        .selection_generation;

    let reselected = registry
        .select_lobby_link_cable_game(
            invite,
            guest_connection,
            link_game("Guest GBA v1 replacement", "gba", 'c'),
            LobbyLinkProtocolFamily::GbaMultiV1,
            None,
        )
        .await
        .expect("guest reselected in authoritative GBA v1 family");
    let link = reselected
        .multiplayer_extension
        .as_ref()
        .and_then(|extension| extension.link_cable.as_ref())
        .expect("GBA v1 extension");

    assert_eq!(link.protocol_family, LobbyLinkProtocolFamily::GbaMultiV1);
    assert!(link.players[1].selection_generation > first_generation);
    assert_eq!(
        link.players[1]
            .selected_game
            .as_ref()
            .expect("replacement game")
            .title,
        "Guest GBA v1 replacement"
    );
}

#[tokio::test]
async fn rotating_link_room_invalidates_every_occupied_player_launch_generation() {
    let (registry, invite, host_connection, guest_connection, _, selected) =
        selected_link_lobby().await;
    let before = selected.multiplayer_extension.as_ref().expect("extension");
    let before_link = before.link_cable.as_ref().expect("link");
    let before_host_generation = before_link.players[0].selection_generation;
    let before_guest_generation = before_link.players[1].selection_generation;

    registry
        .set_lobby_link_cable_launch_state(
            invite.clone(),
            host_connection,
            before_host_generation,
            LobbyLinkCableLaunchState::RuntimeAttached,
            None,
        )
        .await
        .expect("host attached");
    registry
        .set_lobby_link_cable_launch_state(
            invite.clone(),
            guest_connection,
            before_guest_generation,
            LobbyLinkCableLaunchState::RuntimeAttached,
            None,
        )
        .await
        .expect("guest attached");

    let rotated = registry
        .select_lobby_link_cable_game(
            invite.clone(),
            host_connection,
            link_game("Host GBA", "gba", 'a'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            Some(InviteCode::parse("JK67-LM").expect("new room invite")),
        )
        .await
        .expect("room rotated");
    let rotated_extension = rotated.multiplayer_extension.as_ref().expect("extension");
    let rotated_link = rotated_extension.link_cable.as_ref().expect("link");

    assert!(rotated_extension.generation > before.generation);
    assert!(rotated_link.players[0].selection_generation > before_host_generation);
    assert!(rotated_link.players[1].selection_generation > before_guest_generation);
    assert!(
        rotated_link
            .players
            .iter()
            .all(|player| player.launch_state == LobbyLinkCableLaunchState::NotLaunched)
    );

    let stale_guest_update = registry
        .set_lobby_link_cable_launch_state(
            invite,
            guest_connection,
            before_guest_generation,
            LobbyLinkCableLaunchState::RuntimeAttached,
            None,
        )
        .await
        .expect_err("old room generation must not update new room state");
    assert!(matches!(
        stale_guest_update,
        LobbyError::StaleLinkCableSelection
    ));
}

#[tokio::test]
async fn intentional_link_guest_leave_clears_route_and_interrupts_remaining_player() {
    let (registry, invite, host_connection, guest_connection, _, selected) =
        selected_link_lobby().await;
    let selected_extension = selected.multiplayer_extension.as_ref().expect("extension");
    let selected_link = selected_extension.link_cable.as_ref().expect("link");
    let host_generation = selected_link.players[0].selection_generation;
    let guest_generation = selected_link.players[1].selection_generation;

    registry
        .set_lobby_link_cable_launch_state(
            invite.clone(),
            host_connection,
            host_generation,
            LobbyLinkCableLaunchState::RuntimeAttached,
            None,
        )
        .await
        .expect("host attached");
    registry
        .set_lobby_link_cable_launch_state(
            invite.clone(),
            guest_connection,
            guest_generation,
            LobbyLinkCableLaunchState::RuntimeAttached,
            None,
        )
        .await
        .expect("guest attached");

    let left = registry
        .leave_lobby(invite.clone(), guest_connection)
        .await
        .expect("guest left intentionally");
    let left_extension = left.multiplayer_extension.as_ref().expect("extension");
    let left_link = left_extension.link_cable.as_ref().expect("link");

    assert!(left_extension.generation > selected_extension.generation);
    assert!(left_link.room_invite_code.is_none());
    assert!(left_link.cable_epoch.is_none());
    assert!(left_link.players[0].selection_generation > host_generation);
    assert_eq!(
        left_link.players[0].launch_state,
        LobbyLinkCableLaunchState::Interrupted
    );
    assert!(left_link.players[1].selected_game.is_none());
    assert_eq!(
        left_link.players[1].launch_state,
        LobbyLinkCableLaunchState::Stopped
    );

    let replacement_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("replacement"),
            join_link_params(),
            replacement_connection,
        )
        .await
        .expect("replacement joined");
    let replacement_selection = registry
        .select_lobby_link_cable_game(
            invite.clone(),
            replacement_connection,
            link_game("Replacement GBA", "gba", 'c'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            None,
        )
        .await
        .expect("replacement selected");
    let replacement_generation = replacement_selection
        .multiplayer_extension
        .as_ref()
        .and_then(|extension| extension.link_cable.as_ref())
        .expect("link")
        .players[1]
        .selection_generation;
    let no_stale_route = registry
        .set_lobby_link_cable_launch_state(
            invite,
            replacement_connection,
            replacement_generation,
            LobbyLinkCableLaunchState::Launching,
            None,
        )
        .await
        .expect_err("host must publish a fresh direct room");
    assert!(matches!(no_stale_route, LobbyError::GameLaunchNotReady));
}

#[tokio::test]
async fn removing_link_guest_uses_the_same_terminal_route_reset() {
    let (registry, invite, host_connection, _, _, selected) = selected_link_lobby().await;
    let selected_extension = selected.multiplayer_extension.as_ref().expect("extension");
    let selected_link = selected_extension.link_cable.as_ref().expect("link");

    let removed = registry
        .remove_lobby_player(
            invite,
            host_connection,
            selected.lobby_epoch,
            PlayerIndex::TWO,
        )
        .await
        .expect("guest removed");
    let removed_extension = removed.multiplayer_extension.as_ref().expect("extension");
    let removed_link = removed_extension.link_cable.as_ref().expect("link");

    assert!(removed_extension.generation > selected_extension.generation);
    assert!(removed_link.room_invite_code.is_none());
    assert!(
        removed_link.players[0].selection_generation
            > selected_link.players[0].selection_generation
    );
    assert_eq!(
        removed_link.players[0].launch_state,
        LobbyLinkCableLaunchState::Interrupted
    );
}

#[tokio::test]
async fn ordinary_link_disconnect_and_reconnect_preserve_route_and_player_selections() {
    let (registry, invite, _, guest_connection, guest_resume_token, selected) =
        selected_link_lobby().await;
    let selected_extension = selected.multiplayer_extension.as_ref().expect("extension");
    let selected_link = selected_extension.link_cable.as_ref().expect("link");

    let disconnected = registry
        .disconnect_lobby(invite.clone(), guest_connection)
        .await
        .expect("guest disconnected");
    let disconnected_extension = disconnected
        .multiplayer_extension
        .as_ref()
        .expect("extension");
    let disconnected_link = disconnected_extension.link_cable.as_ref().expect("link");

    assert_eq!(
        disconnected_extension.generation,
        selected_extension.generation
    );
    assert_eq!(
        disconnected_link.room_invite_code,
        selected_link.room_invite_code
    );
    assert_eq!(
        disconnected_link.players[1].selected_game,
        selected_link.players[1].selected_game
    );
    assert_eq!(
        disconnected_link.players[1].selection_generation,
        selected_link.players[1].selection_generation
    );

    let reconnected = registry
        .reconnect_lobby_player(ReconnectLobbyPlayerRequest {
            invite_code: invite,
            player: license("guest"),
            params: join_link_params(),
            player_index: PlayerIndex::TWO,
            lobby_epoch: selected.lobby_epoch,
            resume_token: guest_resume_token,
            connection_id: ConnectionId::new(),
        })
        .await
        .expect("guest reconnected");
    let reconnected_link = reconnected
        .lobby
        .multiplayer_extension
        .as_ref()
        .and_then(|extension| extension.link_cable.as_ref())
        .expect("link");

    assert_eq!(
        reconnected_link.room_invite_code,
        selected_link.room_invite_code
    );
    assert_eq!(
        reconnected_link.players[1].selected_game,
        selected_link.players[1].selected_game
    );
}

#[tokio::test]
async fn gb_and_gbc_share_one_link_family_but_gba_does_not() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_link_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_link_params(),
            host_connection,
        )
        .await
        .expect("host connected");
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_link_params(),
            guest_connection,
        )
        .await
        .expect("guest connected");
    registry
        .select_lobby_link_cable_game(
            invite.clone(),
            host_connection,
            link_game("Red", "gb", 'a'),
            LobbyLinkProtocolFamily::GbSerialV1,
            Some(InviteCode::parse("EF45-GH").expect("room invite")),
        )
        .await
        .expect("host selected GB");
    registry
        .select_lobby_link_cable_game(
            invite.clone(),
            guest_connection,
            link_game("Crystal", "gbc", 'b'),
            LobbyLinkProtocolFamily::GbSerialV1,
            None,
        )
        .await
        .expect("guest selected GBC");

    let mismatch = registry
        .select_lobby_link_cable_game(
            invite,
            guest_connection,
            link_game("Emerald", "gba", 'c'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            None,
        )
        .await
        .expect_err("GBA cannot join a GB/GBC link family");
    assert!(matches!(mismatch, LobbyError::LinkCableFamilyMismatch));
}

#[tokio::test]
async fn link_resolution_rejects_legacy_or_third_players_without_changing_controller_capacity() {
    let link_registry = registry();
    let host_join = link_registry
        .create_lobby(license("host"), create_link_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    let legacy_connection = ConnectionId::new();
    link_registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_link_params(),
            host_connection,
        )
        .await
        .expect("host connected");
    link_registry
        .connect_lobby(
            invite.clone(),
            license("legacy"),
            join_params(),
            legacy_connection,
        )
        .await
        .expect("legacy guest connected before resolution");
    let unsupported = link_registry
        .select_lobby_link_cable_game(
            invite.clone(),
            host_connection,
            link_game("Host GBA", "gba", 'a'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            Some(InviteCode::parse("EF45-GH").expect("room invite")),
        )
        .await
        .expect_err("legacy guest blocks link resolution");
    assert!(matches!(
        unsupported,
        LobbyError::LinkCableCapabilityRequired
    ));
    link_registry
        .leave_lobby(invite.clone(), legacy_connection)
        .await
        .expect("legacy guest left");
    link_registry
        .select_lobby_link_cable_game(
            invite.clone(),
            host_connection,
            link_game("Host GBA", "gba", 'a'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            Some(InviteCode::parse("EF45-GH").expect("room invite")),
        )
        .await
        .expect("link resolved");
    link_registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_link_params(),
            ConnectionId::new(),
        )
        .await
        .expect("second link player joined");
    let third = link_registry
        .connect_lobby(
            invite,
            license("third"),
            join_link_params(),
            ConnectionId::new(),
        )
        .await
        .expect_err("link lobby stays two-player");
    assert!(matches!(third, LobbyError::LobbyFull));

    let controller_registry = registry();
    let controller = controller_registry
        .create_lobby(license("controller-host"), create_params())
        .await
        .expect("controller created");
    let controller_invite =
        InviteCode::parse(controller.lobby.invite_code).expect("controller invite");
    for subject in ["controller-2", "controller-3", "controller-4"] {
        controller_registry
            .join_lobby(controller_invite.clone(), license(subject), join_params())
            .await
            .expect("controller capacity unchanged");
    }
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

    registry
        .set_lobby_game_readiness(
            invite.clone(),
            host_connection,
            selected.proposal_id,
            LobbyGameReadinessStatus::Ready,
            None,
        )
        .await
        .expect("host ready");
    let missing_state_view = registry
        .set_lobby_game_readiness(
            invite.clone(),
            guest_connection,
            selected.proposal_id,
            LobbyGameReadinessStatus::MissingStartupState,
            Some("selected state pending".to_owned()),
        )
        .await
        .expect("guest missing selected state");
    assert!(missing_state_view.game_readiness.iter().any(|readiness| {
        readiness.player_index == 1
            && readiness.status == LobbyGameReadinessStatus::MissingStartupState
    }));
    let missing_state = registry
        .request_lobby_game_launch(invite.clone(), host_connection, selected.proposal_id)
        .await
        .expect_err("guest selected state not ready");
    assert!(matches!(missing_state, LobbyError::PlayersNotReady));

    registry
        .set_lobby_game_readiness(
            invite.clone(),
            guest_connection,
            selected.proposal_id,
            LobbyGameReadinessStatus::Ready,
            None,
        )
        .await
        .expect("guest ready");

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

    let published_epoch = published.lobby_epoch;
    let pending_launch = published.pending_launch.clone().expect("launch");

    assert_eq!(
        pending_launch.status,
        crate::lobbies::LobbyGameLaunchStatus::Ready
    );
    assert_eq!(pending_launch.room_invite_code.as_deref(), Some("AB23-CD"));

    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");
    let returned = registry
        .return_lobby_from_game(
            invite.clone(),
            guest_connection,
            published_epoch,
            selected.proposal_id,
            Some(PlayerIndex::TWO),
            Some(LobbyReturnReason::PlayerRequestedReturn),
        )
        .await
        .expect("returned");
    let returned_event = recv_lobby_event(&mut events).await;
    let state_event = recv_lobby_event(&mut events).await;

    assert_eq!(returned.status, LobbyStatus::GameSelected);
    assert!(returned.pending_launch.is_none());
    assert!(returned.game_readiness.is_empty());
    match returned_event {
        LobbyEvent::LobbyReturned {
            lobby,
            returned: returned_metadata,
        } => {
            assert_eq!(lobby.status, LobbyStatus::GameSelected);
            assert_eq!(returned_metadata.proposal_id, selected.proposal_id);
            assert_eq!(returned_metadata.return_requested_by_player_index, Some(1));
            assert_eq!(
                returned_metadata.reason,
                Some(LobbyReturnReason::PlayerRequestedReturn)
            );
        }
        other => panic!("expected lobby returned event, got {other:?}"),
    }
    assert!(matches!(state_event, LobbyEvent::LobbyStateChanged(_)));

    let duplicate = registry
        .return_lobby_from_game(
            invite.clone(),
            host_connection,
            published_epoch,
            selected.proposal_id,
            Some(PlayerIndex::ONE),
            Some(LobbyReturnReason::PlayerRequestedReturn),
        )
        .await
        .expect("duplicate return is idempotent");

    assert_eq!(duplicate.status, LobbyStatus::GameSelected);
    assert!(events.try_recv().is_err());

    let mut next_game = game_candidate();
    next_game.title = "Starlight Ruins II".to_string();
    next_game.content_sha256 = Some("d".repeat(64));
    let next_selection = registry
        .select_lobby_game(invite.clone(), host_connection, next_game)
        .await
        .expect("host can change game after return")
        .selected_game
        .expect("next selection");
    let unready = registry
        .set_lobby_game_readiness(
            invite.clone(),
            host_connection,
            next_selection.proposal_id,
            LobbyGameReadinessStatus::NotReady,
            None,
        )
        .await
        .expect("host can unready after return");
    assert_eq!(unready.status, LobbyStatus::GameSelected);
    for connection in [host_connection, guest_connection] {
        registry
            .set_lobby_game_readiness(
                invite.clone(),
                connection,
                next_selection.proposal_id,
                LobbyGameReadinessStatus::Ready,
                None,
            )
            .await
            .expect("player ready for relaunch");
    }
    let relaunched = registry
        .request_lobby_game_launch(invite, host_connection, next_selection.proposal_id)
        .await
        .expect("host can relaunch after return");

    assert_eq!(relaunched.status, LobbyStatus::InGame);
    assert_eq!(
        relaunched.pending_launch.expect("relaunch").proposal_id,
        next_selection.proposal_id
    );
}

#[tokio::test]
async fn gameplay_started_marks_launch_playing_after_all_v2_players_report() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_v2_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_v2_params(),
            host_connection,
        )
        .await
        .expect("host connected");
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_v2_params(),
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

    let launch = registry
        .request_lobby_game_launch(invite.clone(), host_connection, selected.proposal_id)
        .await
        .expect("launch requested");
    let not_ready = registry
        .mark_lobby_gameplay_started(
            invite.clone(),
            guest_connection,
            launch.lobby_epoch,
            selected.proposal_id,
        )
        .await
        .expect_err("gameplay cannot start before room is published");
    assert!(matches!(not_ready, LobbyError::GameLaunchNotReady));

    let published = registry
        .publish_lobby_game_room(
            invite.clone(),
            host_connection,
            selected.proposal_id,
            InviteCode::parse("AB23-CD").expect("room invite"),
        )
        .await
        .expect("published");
    let published_epoch = published.lobby_epoch;
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");

    let guest_reported = registry
        .mark_lobby_gameplay_started(
            invite.clone(),
            guest_connection,
            published_epoch,
            selected.proposal_id,
        )
        .await
        .expect("guest started");
    let guest_event = recv_lobby_event(&mut events).await;

    let pending = guest_reported
        .pending_launch
        .as_ref()
        .expect("pending launch");
    assert_eq!(pending.status, LobbyGameLaunchStatus::Ready);
    assert_eq!(pending.started_player_indexes, vec![1]);
    assert!(pending.gameplay_started_at_ms.is_none());
    match guest_event {
        LobbyEvent::LobbyStateChanged(lobby) => {
            let launch = lobby.pending_launch.as_ref().expect("pending launch");
            assert_eq!(launch.status, LobbyGameLaunchStatus::Ready);
            assert_eq!(launch.started_player_indexes, vec![1]);
        }
        other => panic!("expected gameplay state change, got {other:?}"),
    }

    let started = registry
        .mark_lobby_gameplay_started(
            invite.clone(),
            host_connection,
            published_epoch + 1,
            selected.proposal_id,
        )
        .await
        .expect("host started");
    let event = recv_lobby_event(&mut events).await;

    let pending = started.pending_launch.as_ref().expect("pending launch");
    assert_eq!(pending.status, LobbyGameLaunchStatus::Playing);
    assert_eq!(pending.started_player_indexes, vec![0, 1]);
    assert!(pending.gameplay_started_at_ms.is_some());
    match event {
        LobbyEvent::LobbyStateChanged(lobby) => {
            let launch = lobby.pending_launch.as_ref().expect("pending launch");
            assert_eq!(launch.status, LobbyGameLaunchStatus::Playing);
            assert_eq!(launch.started_player_indexes, vec![0, 1]);
        }
        other => panic!("expected gameplay state change, got {other:?}"),
    }
    assert!(
        registry
            .lobby_events(invite.clone(), 10)
            .await
            .expect("debug events")
            .iter()
            .any(|event| event.kind == "lobbyGameplayStarted")
    );

    let duplicate = registry
        .mark_lobby_gameplay_started(
            invite,
            guest_connection,
            published_epoch + 1,
            selected.proposal_id,
        )
        .await
        .expect("duplicate started is idempotent");

    assert_eq!(
        duplicate.pending_launch.expect("pending launch").status,
        LobbyGameLaunchStatus::Playing
    );
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn gameplay_started_from_legacy_client_leaves_launch_ready() {
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
    registry
        .set_lobby_game_readiness(
            invite.clone(),
            host_connection,
            selected.proposal_id,
            LobbyGameReadinessStatus::Ready,
            None,
        )
        .await
        .expect("ready");
    registry
        .request_lobby_game_launch(invite.clone(), host_connection, selected.proposal_id)
        .await
        .expect("launch requested");
    let published = registry
        .publish_lobby_game_room(
            invite.clone(),
            host_connection,
            selected.proposal_id,
            InviteCode::parse("AB23-CD").expect("room invite"),
        )
        .await
        .expect("published");
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");

    let reported = registry
        .mark_lobby_gameplay_started(
            invite,
            host_connection,
            published.lobby_epoch,
            selected.proposal_id,
        )
        .await
        .expect("legacy report ignored");

    let launch = reported.pending_launch.expect("pending launch");
    assert_eq!(launch.status, LobbyGameLaunchStatus::Ready);
    assert!(launch.started_player_indexes.is_empty());
    assert!(launch.gameplay_started_at_ms.is_none());
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn gameplay_started_with_legacy_peer_leaves_launch_ready() {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_v2_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_v2_params(),
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
        .expect("legacy guest connected");
    let selected = registry
        .select_lobby_game(invite.clone(), host_connection, game_candidate())
        .await
        .expect("selected")
        .selected_game
        .expect("selected game");
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
    registry
        .request_lobby_game_launch(invite.clone(), host_connection, selected.proposal_id)
        .await
        .expect("launch requested");
    let published = registry
        .publish_lobby_game_room(
            invite.clone(),
            host_connection,
            selected.proposal_id,
            InviteCode::parse("AB23-CD").expect("room invite"),
        )
        .await
        .expect("published");
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");

    let reported = registry
        .mark_lobby_gameplay_started(
            invite,
            host_connection,
            published.lobby_epoch,
            selected.proposal_id,
        )
        .await
        .expect("mixed-capability report ignored");

    let launch = reported.pending_launch.expect("pending launch");
    assert_eq!(launch.status, LobbyGameLaunchStatus::Ready);
    assert!(launch.started_player_indexes.is_empty());
    assert!(launch.gameplay_started_at_ms.is_none());
    assert!(events.try_recv().is_err());
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
    let selected_view = registry
        .select_lobby_game(invite.clone(), host_connection, game_candidate())
        .await
        .expect("selected");
    let selected_epoch = selected_view.lobby_epoch;
    let selected = selected_view.selected_game.expect("selected game");

    let error = registry
        .return_lobby_from_game(
            invite,
            host_connection,
            selected_epoch,
            selected.proposal_id,
            Some(PlayerIndex::ONE),
            Some(LobbyReturnReason::PlayerRequestedReturn),
        )
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

fn create_v2_params() -> CreateLobbyParams {
    CreateLobbyParams {
        display_name: Some("Host".to_string()),
        capabilities: v2_lobby_capabilities(),
        initial_game: None,
        voice: None,
        visibility: LobbyVisibility::Private,
    }
}

fn join_params() -> JoinLobbyParams {
    JoinLobbyParams {
        display_name: None,
        capabilities: LobbyClientCapabilities::desktop_default(),
    }
}

fn join_v2_params() -> JoinLobbyParams {
    JoinLobbyParams {
        display_name: None,
        capabilities: v2_lobby_capabilities(),
    }
}

fn create_link_params() -> CreateLobbyParams {
    CreateLobbyParams {
        display_name: Some("Host".to_string()),
        capabilities: link_lobby_capabilities(),
        initial_game: None,
        voice: None,
        visibility: LobbyVisibility::Private,
    }
}

fn join_link_params() -> JoinLobbyParams {
    JoinLobbyParams {
        display_name: None,
        capabilities: link_lobby_capabilities(),
    }
}

fn link_lobby_capabilities() -> LobbyClientCapabilities {
    LobbyClientCapabilities {
        link_cable: Some(LobbyLinkCableClientCapabilities {
            contract_version: 1,
            runtime_profile: "mgba-link-runtime-v1".to_string(),
            core_build_id: "android-mgba-link-v1".to_string(),
            protocol_families: vec![
                LobbyLinkProtocolFamily::GbSerialV1,
                LobbyLinkProtocolFamily::GbaMultiV1,
                LobbyLinkProtocolFamily::GbaMultiV2,
            ],
        }),
        ..LobbyClientCapabilities::desktop_default()
    }
}

fn link_game(title: &str, system_id: &str, hash_byte: char) -> LobbyGameCandidate {
    LobbyGameCandidate {
        title: title.to_string(),
        system_id: system_id.to_string(),
        core_id: "mgba".to_string(),
        content_sha256: Some(hash_byte.to_string().repeat(64)),
        rom_size_bytes: Some(1024),
        start_state_label: None,
    }
}

async fn selected_link_lobby() -> (
    InMemoryLobbyRegistry,
    InviteCode,
    ConnectionId,
    ConnectionId,
    String,
    LobbyView,
) {
    let registry = registry();
    let host_join = registry
        .create_lobby(license("host"), create_link_params())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_link_params(),
            host_connection,
        )
        .await
        .expect("host connected");
    let guest_join = registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_link_params(),
            guest_connection,
        )
        .await
        .expect("guest connected");
    registry
        .select_lobby_link_cable_game(
            invite.clone(),
            host_connection,
            link_game("Host GBA", "gba", 'a'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            Some(InviteCode::parse("EF45-GH").expect("room invite")),
        )
        .await
        .expect("host selected");
    let selected = registry
        .select_lobby_link_cable_game(
            invite.clone(),
            guest_connection,
            link_game("Guest GBA", "gba", 'b'),
            LobbyLinkProtocolFamily::GbaMultiV2,
            None,
        )
        .await
        .expect("guest selected");

    (
        registry,
        invite,
        host_connection,
        guest_connection,
        guest_join.resume_token,
        selected,
    )
}

fn v2_lobby_capabilities() -> LobbyClientCapabilities {
    LobbyClientCapabilities {
        supports_lobby_returned_event: true,
        supports_lobby_gameplay_started: true,
        ..LobbyClientCapabilities::desktop_default()
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
