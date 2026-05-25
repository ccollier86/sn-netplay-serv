//! Tests for persistent lobby registry behavior.

use crate::auth::{ClientKind, VerifiedLicense};
use crate::lobbies::{
    CreateLobbyParams, InMemoryLobbyRegistry, JoinLobbyParams, LobbyClientCapabilities, LobbyError,
    LobbyPlayerRole, LobbyRegistry,
};
use crate::rooms::{InviteCode, InviteCodeGenerator, ResumeToken, ResumeTokenGenerator};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

fn registry() -> InMemoryLobbyRegistry {
    InMemoryLobbyRegistry::with_generators(
        Arc::new(SequenceInviteCodeGenerator::default()),
        Arc::new(SequenceResumeTokenGenerator::default()),
    )
}

fn create_params() -> CreateLobbyParams {
    CreateLobbyParams {
        display_name: Some("Host".to_string()),
        capabilities: LobbyClientCapabilities::desktop_default(),
        initial_game: None,
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
