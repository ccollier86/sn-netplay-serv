//! Tests for netplay room voice-broker lifecycle wiring.
//!
//! Voice grants are private per player. These tests keep that contract separate
//! from the broad room-registry behavior suite.

use super::{InMemoryRoomRegistry, RoomRegistry};
use crate::auth::VerifiedLicense;
use crate::protocol::NetplaySessionDescriptor;
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, NoopRoomDebugEventSink, RoomRecoveryConfig,
    RoomVoiceStatus, SystemClock, UuidResumeTokenGenerator,
};
use crate::voice::{
    CreateVoiceRoomBrokerRequest, CreateVoiceRoomBrokerResponse, IssueVoiceTokenBrokerRequest,
    VoiceBroker, VoiceBrokerError, VoiceBrokerGrant, VoiceBrokerRoomView,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct StaticInviteCodeGenerator;

impl InviteCodeGenerator for StaticInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        InviteCode::parse("AB23-CD").expect("invite")
    }
}

#[tokio::test]
async fn voice_enabled_room_exposes_shared_metadata_without_token() {
    let registry = registry_with_voice_broker(MockVoiceBroker::available());
    let view = registry
        .create_room(
            license("host"),
            ConnectionId::new(),
            descriptor_with_voice(),
        )
        .await
        .expect("room");

    assert_eq!(
        view.voice.as_ref().map(|voice| voice.status),
        Some(RoomVoiceStatus::Available)
    );
    assert_eq!(
        view.voice
            .as_ref()
            .and_then(|voice| voice.server_url.as_deref()),
        Some("wss://voice.shadowboy.app")
    );

    let json = serde_json::to_string(&view).expect("room view json");
    assert!(!json.contains("token-host"));
    assert!(!json.contains("token-guest"));
}

#[tokio::test]
async fn voice_join_returns_only_the_matching_player_grant() {
    let registry = registry_with_voice_broker(MockVoiceBroker::available());
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let view = registry
        .create_room(
            license("host"),
            ConnectionId::new(),
            descriptor_with_voice(),
        )
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");

    let host_join = registry
        .connect_host(invite.clone(), license("host"), host_connection)
        .await
        .expect("host join");
    let guest_join = registry
        .connect_guest(invite, license("guest"), guest_connection)
        .await
        .expect("guest join");

    assert_eq!(
        host_join.voice.as_ref().map(|voice| voice.token.as_str()),
        Some("token-host")
    );
    assert_eq!(
        guest_join.voice.as_ref().map(|voice| voice.token.as_str()),
        Some("token-guest")
    );
    assert_ne!(host_join.voice, guest_join.voice);
}

#[tokio::test]
async fn voice_broker_failure_keeps_room_playable_without_grants() {
    let registry = registry_with_voice_broker(MockVoiceBroker::failing());
    let view = registry
        .create_room(
            license("host"),
            ConnectionId::new(),
            descriptor_with_voice(),
        )
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");

    let guest_join = registry
        .connect_guest(invite, license("guest"), ConnectionId::new())
        .await
        .expect("guest join");

    assert_eq!(
        view.voice.as_ref().map(|voice| voice.status),
        Some(RoomVoiceStatus::Unavailable)
    );
    assert!(guest_join.voice.is_none());
}

#[tokio::test]
async fn voice_room_is_closed_when_player_exits() {
    let broker = MockVoiceBroker::available();
    let closed = broker.closed.clone();
    let registry = registry_with_voice_broker(broker);
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let view = registry
        .create_room(
            license("host"),
            ConnectionId::new(),
            descriptor_with_voice(),
        )
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");
    registry
        .connect_host(invite.clone(), license("host"), host_connection)
        .await
        .expect("host join");
    registry
        .connect_guest(invite.clone(), license("guest"), guest_connection)
        .await
        .expect("guest join");

    registry
        .player_exited(invite, guest_connection, "userQuit".to_string())
        .await
        .expect("player exit");

    for _ in 0..10 {
        if closed.lock().expect("closed rooms").len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(
        closed.lock().expect("closed rooms").as_slice(),
        ["voice-room-1"]
    );
}

#[tokio::test]
async fn voice_token_refresh_updates_only_requesting_player_grant() {
    let registry = registry_with_voice_broker(MockVoiceBroker::available());
    let host_connection = ConnectionId::new();
    let guest_connection = ConnectionId::new();
    let view = registry
        .create_room(
            license("host"),
            ConnectionId::new(),
            descriptor_with_voice(),
        )
        .await
        .expect("room");
    let invite = InviteCode::parse(view.invite_code).expect("invite");
    registry
        .connect_host(invite.clone(), license("host"), host_connection)
        .await
        .expect("host join");
    let guest_join = registry
        .connect_guest(invite.clone(), license("guest"), guest_connection)
        .await
        .expect("guest join");

    let refresh = registry
        .refresh_voice_token(invite, guest_connection)
        .await
        .expect("voice refresh");

    assert_eq!(
        guest_join.voice.as_ref().map(|voice| voice.token.as_str()),
        Some("token-guest")
    );
    assert_eq!(refresh.voice.token, "refreshed-player-2");
    assert_eq!(refresh.voice.participant_identity, "player-2");
    assert_eq!(
        refresh.room.voice.and_then(|voice| voice.voice_room_id),
        Some("voice-room-1".to_string())
    );
}

fn registry_with_voice_broker(broker: MockVoiceBroker) -> InMemoryRoomRegistry {
    InMemoryRoomRegistry::with_dependencies_event_sink_and_voice(
        Arc::new(StaticInviteCodeGenerator),
        Arc::new(UuidResumeTokenGenerator),
        Arc::new(SystemClock),
        RoomRecoveryConfig::default(),
        Arc::new(NoopRoomDebugEventSink),
        Arc::new(broker),
    )
}

fn license(subject_id: &str) -> VerifiedLicense {
    VerifiedLicense::new(subject_id, "premium", vec!["netplay".to_string()])
}

fn descriptor_with_voice() -> NetplaySessionDescriptor {
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
        "voice": {
            "enabled": true,
            "mode": "voiceActivation"
        }
    }))
    .expect("descriptor")
}

#[derive(Clone)]
struct MockVoiceBroker {
    fail_create: bool,
    closed: Arc<Mutex<Vec<String>>>,
}

impl MockVoiceBroker {
    fn available() -> Self {
        Self {
            fail_create: false,
            closed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn failing() -> Self {
        Self {
            fail_create: true,
            closed: Arc::new(Mutex::new(Vec::new())),
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
        if self.fail_create {
            return Err(VoiceBrokerError::RequestFailed);
        }

        Ok(CreateVoiceRoomBrokerResponse {
            room: VoiceBrokerRoomView {
                voice_room_id: "voice-room-1".to_string(),
                livekit_room_name: "sb-voice-room-1".to_string(),
                server_url: "wss://voice.shadowboy.app".to_string(),
                netplay_room_id: request.netplay_room_id,
                netplay_invite_code: request.netplay_invite_code,
                room_epoch: request.room_epoch,
                mode: request.mode,
                max_participants: request.max_participants,
            },
            grants: vec![
                VoiceBrokerGrant {
                    player_index: 1,
                    participant_identity: "player-1".to_string(),
                    token: "token-host".to_string(),
                    expires_at: "2026-05-23T20:00:00Z".to_string(),
                },
                VoiceBrokerGrant {
                    player_index: 2,
                    participant_identity: "player-2".to_string(),
                    token: "token-guest".to_string(),
                    expires_at: "2026-05-23T20:00:00Z".to_string(),
                },
            ],
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
            expires_at: "2026-05-23T21:00:00Z".to_string(),
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
