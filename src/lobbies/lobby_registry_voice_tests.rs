//! Tests for lobby-scoped voice-broker lifecycle wiring.
//!
//! Lobby voice persists across launched games, so private grants must be tied to
//! lobby membership instead of active game-room sockets.

use crate::auth::{ClientKind, VerifiedLicense};
use crate::lobbies::{
    CreateLobbyParams, InMemoryLobbyRegistry, JoinLobbyParams, LobbyClientCapabilities,
    LobbyRegistry, LobbyServerCapabilities, MAX_LOBBY_PLAYERS,
};
use crate::protocol::{NetplayVoiceDescriptor, NetplayVoiceMode};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, ResumeToken, ResumeTokenGenerator,
    RoomVoiceStatus,
};
use crate::voice::{
    CreateVoiceRoomBrokerRequest, CreateVoiceRoomBrokerResponse, IssueVoiceTokenBrokerRequest,
    VoiceBroker, VoiceBrokerError, VoiceBrokerGrant, VoiceBrokerRoomView,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn lobby_voice_exposes_shared_metadata_and_private_host_grant() {
    let registry = registry_with_voice(MockVoiceBroker::available());

    let join = registry
        .create_lobby(license("host"), create_params_with_voice())
        .await
        .expect("created");

    assert_eq!(
        join.lobby.voice.as_ref().map(|voice| voice.status),
        Some(RoomVoiceStatus::Available)
    );
    assert_eq!(
        join.lobby
            .voice
            .as_ref()
            .and_then(|voice| voice.server_url.as_deref()),
        Some("wss://voice.shadowboy.app")
    );
    assert_eq!(
        join.voice.as_ref().map(|voice| voice.token.as_str()),
        Some("token-player-1")
    );

    let json = serde_json::to_string(&join.lobby).expect("lobby json");
    assert!(!json.contains("token-player-1"));
    assert!(!json.contains("token-player-2"));
}

#[tokio::test]
async fn lobby_voice_join_returns_only_joining_player_grant() {
    let registry = registry_with_voice(MockVoiceBroker::available());
    let host_join = registry
        .create_lobby(license("host"), create_params_with_voice())
        .await
        .expect("created");
    let invite = InviteCode::parse(host_join.lobby.invite_code).expect("invite");

    let guest_join = registry
        .join_lobby(invite, license("guest"), join_params())
        .await
        .expect("joined");

    assert_eq!(guest_join.player_index.display_number(), 2);
    assert_eq!(
        guest_join.voice.as_ref().map(|voice| voice.token.as_str()),
        Some("token-player-2")
    );
    assert_ne!(host_join.voice, guest_join.voice);
}

#[tokio::test]
async fn lobby_voice_token_refresh_updates_requesting_player_grant() {
    let registry = registry_with_voice(MockVoiceBroker::available());
    let host_join = registry
        .create_lobby(license("host"), create_params_with_voice())
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

    let refresh = registry
        .refresh_lobby_voice_token(invite, host_connection)
        .await
        .expect("refreshed");

    assert_eq!(refresh.voice.token, "refreshed-player-1");
    assert_eq!(refresh.voice.participant_identity, "lobby-player-1");
    assert!(refresh.event_seq >= 2);
}

#[tokio::test]
async fn lobby_voice_retries_two_player_contract_when_broker_rejects_full_lobby_limit() {
    let broker = MockVoiceBroker::limited_to(2);
    let requests = broker.requests.clone();
    let registry = registry_with_voice(broker);

    let join = registry
        .create_lobby(license("host"), create_params_with_voice())
        .await
        .expect("created");

    assert_eq!(
        join.lobby.voice.as_ref().map(|voice| voice.status),
        Some(RoomVoiceStatus::Available)
    );
    assert_eq!(
        join.lobby
            .voice
            .as_ref()
            .map(|voice| voice.max_participants),
        Some(2)
    );
    assert_eq!(
        requests.lock().expect("voice requests").as_slice(),
        [MAX_LOBBY_PLAYERS, 2]
    );
}

#[tokio::test]
async fn lobby_voice_room_closes_when_host_closes_lobby() {
    let broker = MockVoiceBroker::available();
    let closed = broker.closed.clone();
    let registry = registry_with_voice(broker);
    let host_join = registry
        .create_lobby(license("host"), create_params_with_voice())
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

    registry
        .leave_lobby(invite, host_connection)
        .await
        .expect("left");

    for _ in 0..10 {
        if closed.lock().expect("closed rooms").len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(
        closed.lock().expect("closed rooms").as_slice(),
        ["lobby-voice-room-1"]
    );
}

fn registry_with_voice(broker: MockVoiceBroker) -> InMemoryLobbyRegistry {
    InMemoryLobbyRegistry::with_generators_capabilities_and_voice(
        Arc::new(SequenceInviteCodeGenerator::default()),
        Arc::new(SequenceResumeTokenGenerator::default()),
        LobbyServerCapabilities::current(MAX_LOBBY_PLAYERS, false, true),
        Arc::new(broker),
    )
}

fn create_params_with_voice() -> CreateLobbyParams {
    CreateLobbyParams {
        display_name: Some("Host".to_string()),
        capabilities: LobbyClientCapabilities::desktop_default(),
        initial_game: None,
        voice: Some(NetplayVoiceDescriptor {
            enabled: true,
            mode: NetplayVoiceMode::VoiceActivation,
        }),
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

#[derive(Clone)]
struct MockVoiceBroker {
    closed: Arc<Mutex<Vec<String>>>,
    max_supported_participants: u8,
    requests: Arc<Mutex<Vec<u8>>>,
}

impl MockVoiceBroker {
    fn available() -> Self {
        Self {
            closed: Arc::new(Mutex::new(Vec::new())),
            max_supported_participants: MAX_LOBBY_PLAYERS,
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn limited_to(max_supported_participants: u8) -> Self {
        Self {
            closed: Arc::new(Mutex::new(Vec::new())),
            max_supported_participants,
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl VoiceBroker for MockVoiceBroker {
    fn is_enabled(&self) -> bool {
        true
    }

    async fn create_room(
        &self,
        request: CreateVoiceRoomBrokerRequest,
    ) -> Result<CreateVoiceRoomBrokerResponse, VoiceBrokerError> {
        self.requests
            .lock()
            .expect("voice requests")
            .push(request.max_participants);
        if request.max_participants > self.max_supported_participants {
            return Err(VoiceBrokerError::UnexpectedStatus(400));
        }

        let max_participants = request.max_participants;
        Ok(CreateVoiceRoomBrokerResponse {
            room: VoiceBrokerRoomView {
                voice_room_id: "lobby-voice-room-1".to_string(),
                livekit_room_name: "sb-lobby-voice-room-1".to_string(),
                server_url: "wss://voice.shadowboy.app".to_string(),
                netplay_room_id: request.netplay_room_id,
                netplay_invite_code: request.netplay_invite_code,
                room_epoch: request.room_epoch,
                mode: request.mode,
                max_participants: request.max_participants,
            },
            grants: (1..=max_participants)
                .map(|player_index| VoiceBrokerGrant {
                    player_index,
                    participant_identity: format!("lobby-player-{player_index}"),
                    token: format!("token-player-{player_index}"),
                    expires_at: "2026-05-25T20:00:00Z".to_string(),
                })
                .collect(),
        })
    }

    async fn issue_token(
        &self,
        _voice_room_id: &str,
        request: IssueVoiceTokenBrokerRequest,
    ) -> Result<VoiceBrokerGrant, VoiceBrokerError> {
        Ok(VoiceBrokerGrant {
            player_index: request.player_index,
            participant_identity: request.participant_identity,
            token: format!("refreshed-player-{}", request.player_index),
            expires_at: "2026-05-25T21:00:00Z".to_string(),
        })
    }

    async fn close_room(&self, voice_room_id: &str, _reason: &str) -> Result<(), VoiceBrokerError> {
        self.closed
            .lock()
            .expect("closed rooms")
            .push(voice_room_id.to_string());
        Ok(())
    }
}
