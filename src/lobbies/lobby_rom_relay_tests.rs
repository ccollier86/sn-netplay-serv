use crate::auth::{ClientKind, VerifiedLicense};
use crate::lobbies::{
    CreateLobbyParams, InMemoryLobbyRegistry, JoinLobbyParams, LobbyClientCapabilities, LobbyEvent,
    LobbyRegistry, LobbyRomRelayLimits, LobbyServerCapabilities, LobbyVisibility,
    MAX_LOBBY_PLAYERS,
};
use crate::protocol::{
    LobbyFileRelayGrant, LobbyFileRelayGrantPair, LobbyFileRelayGrantRole,
    LobbyFileRelayMaterialKind,
};
use crate::rooms::{
    ConnectionId, InviteCode, PlayerIndex, UuidInviteCodeGenerator, UuidResumeTokenGenerator,
};
use std::sync::Arc;

#[tokio::test]
async fn host_can_prepare_and_grant_private_rom_transfer() {
    let registry = InMemoryLobbyRegistry::with_generators_and_capabilities(
        Arc::new(UuidInviteCodeGenerator),
        Arc::new(UuidResumeTokenGenerator),
        LobbyServerCapabilities::current(MAX_LOBBY_PLAYERS, true, false),
    );
    let created = registry
        .create_lobby(license("host"), create_params())
        .await
        .expect("lobby");
    let invite = InviteCode::parse(created.lobby.invite_code).expect("invite");
    let mut events = registry
        .subscribe_lobby(invite.clone())
        .await
        .expect("events");
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();

    registry
        .connect_lobby(
            invite.clone(),
            license("host"),
            join_params("Host"),
            host_connection,
        )
        .await
        .expect("host connected");
    registry
        .connect_lobby(
            invite.clone(),
            license("guest"),
            join_params("Guest"),
            guest_connection,
        )
        .await
        .expect("guest connected");
    let selected_game = registry
        .lobby_view(invite.clone())
        .await
        .expect("view")
        .selected_game
        .expect("game");

    let intent = registry
        .prepare_lobby_rom_relay_transfer(
            invite.clone(),
            host_connection,
            selected_game.proposal_id,
            PlayerIndex::TWO,
            LobbyRomRelayLimits { max_bytes: 100_000 },
        )
        .await
        .expect("intent");

    assert_eq!(intent.sha256, "a".repeat(64));
    assert_eq!(intent.size_bytes, 32_768);

    registry
        .grant_lobby_rom_relay_transfer(invite, intent, grants(selected_game.proposal_id))
        .await
        .expect("granted");

    assert!(matches!(
        recv_target_event(&mut events).await,
        LobbyEvent::RomTransferUploadGranted { source, .. } if source == host_connection
    ));
    assert!(matches!(
        recv_target_event(&mut events).await,
        LobbyEvent::RomTransferDownloadReady { receiver, .. } if receiver == guest_connection
    ));
}

fn create_params() -> CreateLobbyParams {
    CreateLobbyParams {
        capabilities: capabilities(),
        display_name: Some("Host".to_string()),
        initial_game: Some(crate::lobbies::LobbyGameCandidate {
            title: "Starlight Ruins".to_string(),
            system_id: "snes".to_string(),
            core_id: "snes9x".to_string(),
            content_sha256: Some("a".repeat(64)),
            rom_size_bytes: Some(32_768),
            start_state_label: None,
        }),
        voice: None,
        visibility: LobbyVisibility::Private,
    }
}

fn join_params(display_name: &str) -> JoinLobbyParams {
    JoinLobbyParams {
        capabilities: capabilities(),
        display_name: Some(display_name.to_string()),
    }
}

fn capabilities() -> LobbyClientCapabilities {
    LobbyClientCapabilities {
        supports_lobby: true,
        supports_lobby_voice: true,
        supports_multi_game_lobby: true,
        supports_lobby_returned_event: true,
        supports_lobby_gameplay_started: true,
        supports_temporary_session_rom_relay: true,
    }
}

fn grants(proposal_id: uuid::Uuid) -> LobbyFileRelayGrantPair {
    LobbyFileRelayGrantPair {
        upload: grant("upload-token", LobbyFileRelayGrantRole::Upload, proposal_id),
        download: grant(
            "download-token",
            LobbyFileRelayGrantRole::Download,
            proposal_id,
        ),
    }
}

fn grant(
    token: &str,
    role: LobbyFileRelayGrantRole,
    proposal_id: uuid::Uuid,
) -> LobbyFileRelayGrant {
    LobbyFileRelayGrant {
        transfer_id: "transfer-1".to_string(),
        relay_url: "https://relay.shadowboy.app".to_string(),
        token: token.to_string(),
        role,
        material_kind: LobbyFileRelayMaterialKind::Game,
        proposal_id,
        sender_player_index: 0,
        receiver_player_index: 1,
        sha256: "a".repeat(64),
        size_bytes: 32_768,
        chunk_size_bytes: 16_384,
        chunk_count: 2,
        expires_at: "2026-05-25T00:00:00Z".to_string(),
        startup_state: None,
    }
}

async fn recv_target_event(events: &mut crate::lobbies::LobbyEventReceiver) -> LobbyEvent {
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
            .await
            .expect("event")
            .expect("event");

        if matches!(
            event,
            LobbyEvent::RomTransferUploadGranted { .. }
                | LobbyEvent::RomTransferDownloadReady { .. }
        ) {
            return event;
        }
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
