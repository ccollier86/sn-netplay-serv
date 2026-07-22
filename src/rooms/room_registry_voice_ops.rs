//! Voice broker orchestration for the in-memory room registry.
//!
//! This module keeps external voice-service IO out of room lifecycle helpers.
//! Failures are non-fatal: gameplay rooms still exist when voice setup fails.

use super::InMemoryRoomRegistry;
use crate::protocol::NetplayVoiceMode;
use crate::rooms::{
    ConnectionId, InviteCode, NetplayRoom, PlayerIndex, PlayerVoiceJoinGrant, RoomError,
    RoomVoiceState, RoomVoiceStatus, RoomVoiceTokenRefresh, RoomVoiceView,
};
use crate::voice::{
    CreateVoiceRoomBrokerRequest, CreateVoiceRoomBrokerResponse, IssueVoiceTokenBrokerRequest,
    VoiceBrokerGrant, VoiceBrokerParticipant,
};
use std::collections::HashMap;
use tracing::warn;

impl InMemoryRoomRegistry {
    /// Creates broker voice state for a new room when the session requested it.
    pub(super) async fn create_voice_state_for_room(
        &self,
        room: &NetplayRoom,
    ) -> Option<RoomVoiceState> {
        if !room.voice_requested() {
            return None;
        }

        let mode = room.requested_voice_mode();
        if !self.voice_broker.is_enabled() {
            return Some(RoomVoiceState::unavailable(
                mode,
                "Voice chat is not available for this room.",
            ));
        }

        match self
            .voice_broker
            .create_room(create_voice_request(room))
            .await
        {
            Ok(response) => Some(voice_state_from_response(response, room.max_players())),
            Err(error) => {
                warn!(
                    room_id = %room.room_id(),
                    invite_code = %room.invite_code().display(),
                    error = %error,
                    "voice broker room creation failed"
                );
                Some(RoomVoiceState::unavailable(
                    mode,
                    "Voice chat is not available for this room.",
                ))
            }
        }
    }

    /// Closes an attached voice room without blocking room cleanup.
    pub(super) fn cleanup_voice_room(&self, voice_room_id: Option<String>, reason: &'static str) {
        let Some(voice_room_id) = voice_room_id else {
            return;
        };
        if !self.voice_broker.is_enabled() {
            return;
        }

        let broker = self.voice_broker.clone();
        tokio::spawn(async move {
            if let Err(error) = broker.close_room(&voice_room_id, reason).await {
                warn!(
                    voice_room_id = %voice_room_id,
                    error = %error,
                    "voice broker room cleanup failed"
                );
            }
        });
    }

    /// Refreshes the private voice token for one connected player.
    pub(super) async fn refresh_voice_token_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomVoiceTokenRefresh, RoomError> {
        if !self.voice_broker.is_enabled() {
            return Err(RoomError::VoiceUnavailable);
        }

        let request = {
            let rooms = self.invite_codes.read().await;
            let stored_room = rooms
                .get(invite_code.normalized())
                .ok_or(RoomError::NotFound)?;
            stored_room
                .room
                .voice_token_refresh_request(connection_id)?
        };

        let grant = self
            .voice_broker
            .issue_token(
                &request.voice_room_id,
                IssueVoiceTokenBrokerRequest {
                    player_index: request.player_index.display_number(),
                    participant_identity: request.participant_identity,
                    display_name: Some(request.display_name),
                },
            )
            .await
            .map_err(|error| {
                warn!(
                    voice_room_id = %request.voice_room_id,
                    error = %error,
                    "voice broker token refresh failed"
                );
                RoomError::VoiceUnavailable
            })?;

        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let voice = stored_room.room.refresh_voice_grant(
            connection_id,
            grant.participant_identity,
            grant.token,
            grant.expires_at,
        )?;
        let room = stored_room.view(now);

        Ok(RoomVoiceTokenRefresh { voice, room })
    }
}

fn create_voice_request(room: &NetplayRoom) -> CreateVoiceRoomBrokerRequest {
    CreateVoiceRoomBrokerRequest {
        netplay_room_id: room.room_id().to_string(),
        netplay_invite_code: room.invite_code().display(),
        room_epoch: room.view().room_epoch,
        mode: room.requested_voice_mode(),
        max_participants: room.max_players(),
        participants: voice_participants(room),
    }
}

fn voice_participants(room: &NetplayRoom) -> Vec<VoiceBrokerParticipant> {
    (0..room.max_players())
        .filter_map(|index| PlayerIndex::new(index, room.max_players()))
        .map(|player_index| VoiceBrokerParticipant {
            player_index: player_index.display_number(),
            participant_identity: participant_identity(room, player_index),
            display_name: format!("Player {}", player_index.display_number()),
        })
        .collect()
}

fn participant_identity(room: &NetplayRoom, player_index: PlayerIndex) -> String {
    format!("room-{}-p{}", room.room_id(), player_index.display_number())
}

fn voice_state_from_response(
    response: CreateVoiceRoomBrokerResponse,
    max_players: u8,
) -> RoomVoiceState {
    let mode = response.room.mode;
    let shared = RoomVoiceView {
        status: RoomVoiceStatus::Available,
        provider: Some("livekit".to_string()),
        voice_room_id: Some(response.room.voice_room_id.clone()),
        livekit_room_name: Some(response.room.livekit_room_name.clone()),
        server_url: Some(response.room.server_url.clone()),
        mode,
        max_participants: response.room.max_participants,
        status_detail: None,
    };
    let grants = response
        .grants
        .into_iter()
        .filter_map(|grant| player_grant(grant, &shared, max_players, mode))
        .collect::<HashMap<_, _>>();

    RoomVoiceState::available(shared, grants)
}

fn player_grant(
    grant: VoiceBrokerGrant,
    shared: &RoomVoiceView,
    max_players: u8,
    mode: NetplayVoiceMode,
) -> Option<(PlayerIndex, PlayerVoiceJoinGrant)> {
    let zero_based = grant.player_index.checked_sub(1)?;
    let player_index = PlayerIndex::new(zero_based, max_players)?;

    Some((
        player_index,
        PlayerVoiceJoinGrant {
            provider: shared.provider.clone()?,
            voice_room_id: shared.voice_room_id.clone()?,
            livekit_room_name: shared.livekit_room_name.clone()?,
            server_url: shared.server_url.clone()?,
            participant_identity: grant.participant_identity,
            token: grant.token,
            expires_at: grant.expires_at,
            mode,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{participant_identity, voice_state_from_response};
    use crate::protocol::{NetplaySessionDescriptor, NetplayVoiceMode};
    use crate::rooms::{NetplayRoom, PlayerIndex};
    use crate::voice::{CreateVoiceRoomBrokerResponse, VoiceBrokerGrant, VoiceBrokerRoomView};

    #[test]
    fn maps_broker_grants_to_zero_based_players() {
        let state = voice_state_from_response(
            CreateVoiceRoomBrokerResponse {
                room: VoiceBrokerRoomView {
                    voice_room_id: "voice-1".to_string(),
                    livekit_room_name: "sb-voice-1".to_string(),
                    server_url: "wss://livekit.shadowboy.app".to_string(),
                    netplay_room_id: "room-1".to_string(),
                    netplay_invite_code: "AB23-CD".to_string(),
                    room_epoch: 1,
                    mode: NetplayVoiceMode::PushToTalk,
                    max_participants: 2,
                },
                grants: vec![
                    grant(1, "token-1"),
                    grant(2, "token-2"),
                    grant(5, "ignored"),
                ],
            },
            2,
        );

        assert_eq!(
            state.grant_for(PlayerIndex::ONE).expect("host grant").token,
            "token-1"
        );
        assert_eq!(
            state
                .grant_for(PlayerIndex::TWO)
                .expect("guest grant")
                .token,
            "token-2"
        );
        assert!(state.view().voice_room_id.is_some());
    }

    #[test]
    fn participant_identity_uses_safe_characters() {
        let room = NetplayRoom::new(
            crate::auth::VerifiedLicense::new("host", "premium", vec!["netplay".to_string()]),
            crate::rooms::ConnectionId::new(),
            crate::rooms::InviteCode::parse("AB23-CD").expect("invite"),
            descriptor(),
        );

        let identity = participant_identity(&room, PlayerIndex::ONE);

        assert!(identity.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        }));
    }

    fn grant(player_index: u8, token: &str) -> VoiceBrokerGrant {
        VoiceBrokerGrant {
            player_index,
            participant_identity: format!("player-{player_index}"),
            token: token.to_string(),
            expires_at: "2026-05-23T20:00:00Z".to_string(),
        }
    }

    fn descriptor() -> NetplaySessionDescriptor {
        serde_json::from_value(serde_json::json!({
            "game": {
                "systemId": "snes",
                "title": "Test Game",
                "romSha256": "a".repeat(64),
                "contentKey": "snes-test-game"
            },
            "core": {
                "coreId": "snes9x",
                "stateFormat": "snes9x:snes:s9x-freeze-stream-v1"
            },
            "voice": {
                "enabled": true,
                "mode": "voiceActivation"
            }
        }))
        .expect("descriptor")
    }
}
