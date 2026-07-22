//! Voice broker orchestration for persistent lobbies.
//!
//! Lobby voice lives above child game rooms. Failures remain non-fatal so the
//! lobby can still be used for chat, game selection, and launching.

use super::InMemoryLobbyRegistry;
use crate::lobbies::{Lobby, LobbyError, LobbyVoiceTokenRefresh, MAX_LOBBY_PLAYERS};
use crate::protocol::{NetplayVoiceDescriptor, NetplayVoiceMode};
use crate::rooms::{
    ConnectionId, InviteCode, PlayerIndex, PlayerVoiceJoinGrant, RoomVoiceState, RoomVoiceStatus,
    RoomVoiceView,
};
use crate::voice::{
    CreateVoiceRoomBrokerRequest, CreateVoiceRoomBrokerResponse, IssueVoiceTokenBrokerRequest,
    VoiceBrokerError, VoiceBrokerGrant, VoiceBrokerParticipant,
};
use std::collections::HashMap;
use tracing::warn;

impl InMemoryLobbyRegistry {
    /// Creates broker voice state for a lobby when the host requested it.
    pub(super) async fn create_voice_state_for_lobby(
        &self,
        lobby: &Lobby,
        voice: Option<&NetplayVoiceDescriptor>,
        client_supports_voice: bool,
    ) -> Option<RoomVoiceState> {
        let descriptor = voice?;
        if !descriptor.enabled || !client_supports_voice || !self.capabilities.supports_lobby_voice
        {
            return None;
        }

        if !self.voice_broker.is_enabled() {
            return Some(RoomVoiceState::unavailable(
                descriptor.mode,
                "Voice chat is not available for this lobby.",
            ));
        }

        match self.create_lobby_voice_room(lobby, descriptor.mode).await {
            Ok(response) => Some(voice_state_from_response(response, descriptor.mode)),
            Err(error) => {
                warn!(
                    lobby_id = %lobby.lobby_id(),
                    invite_code = %lobby.invite_code().display(),
                    error = %error,
                    "voice broker lobby room creation failed"
                );
                Some(RoomVoiceState::unavailable(
                    descriptor.mode,
                    "Voice chat is not available for this lobby.",
                ))
            }
        }
    }

    async fn create_lobby_voice_room(
        &self,
        lobby: &Lobby,
        mode: NetplayVoiceMode,
    ) -> Result<CreateVoiceRoomBrokerResponse, VoiceBrokerError> {
        self.voice_broker
            .create_room(create_lobby_voice_request(lobby, mode, MAX_LOBBY_PLAYERS))
            .await
    }

    /// Refreshes a private lobby voice token for one connected player.
    pub(super) async fn refresh_lobby_voice_token_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<LobbyVoiceTokenRefresh, LobbyError> {
        if !self.voice_broker.is_enabled() {
            return Err(LobbyError::VoiceUnavailable);
        }

        let request = {
            let lobbies = self.lobbies.read().await;
            let lobby = lobbies
                .get(invite_code.normalized())
                .ok_or(LobbyError::NotFound)?;
            lobby.lobby.voice_token_refresh_request(connection_id)?
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
                    "voice broker lobby token refresh failed"
                );
                LobbyError::VoiceUnavailable
            })?;

        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        let voice = lobby.lobby.refresh_voice_grant(
            connection_id,
            grant.participant_identity,
            grant.token,
            grant.expires_at,
        )?;
        let view = lobby.view();

        Ok(LobbyVoiceTokenRefresh {
            event_seq: view.event_seq,
            lobby_epoch: view.lobby_epoch,
            voice,
        })
    }

    /// Closes an attached lobby voice room without blocking lobby cleanup.
    pub(super) fn cleanup_lobby_voice_room(
        &self,
        voice_room_id: Option<String>,
        reason: &'static str,
    ) {
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
                    "voice broker lobby cleanup failed"
                );
            }
        });
    }

    /// Disconnects one removed lobby member from voice without blocking removal.
    pub(super) fn cleanup_lobby_voice_participant(
        &self,
        participant: Option<(String, String)>,
        reason: &'static str,
    ) {
        let Some((voice_room_id, participant_identity)) = participant else {
            return;
        };
        if !self.voice_broker.is_enabled() {
            return;
        }

        let broker = self.voice_broker.clone();
        tokio::spawn(async move {
            if let Err(error) = broker
                .remove_participant(&voice_room_id, &participant_identity, reason)
                .await
            {
                warn!(
                    voice_room_id = %voice_room_id,
                    participant_identity = %participant_identity,
                    error = %error,
                    "voice broker lobby participant cleanup failed"
                );
            }
        });
    }
}

fn create_lobby_voice_request(
    lobby: &Lobby,
    mode: NetplayVoiceMode,
    max_participants: u8,
) -> CreateVoiceRoomBrokerRequest {
    CreateVoiceRoomBrokerRequest {
        netplay_room_id: lobby.lobby_id().to_string(),
        netplay_invite_code: lobby.invite_code().display(),
        room_epoch: lobby.lobby_epoch(),
        mode,
        max_participants,
        participants: voice_participants(lobby, max_participants),
    }
}

fn voice_participants(lobby: &Lobby, max_participants: u8) -> Vec<VoiceBrokerParticipant> {
    let participant_count = max_participants.min(MAX_LOBBY_PLAYERS);

    (0..participant_count)
        .filter_map(|index| PlayerIndex::new(index, MAX_LOBBY_PLAYERS))
        .map(|player_index| VoiceBrokerParticipant {
            player_index: player_index.display_number(),
            participant_identity: participant_identity(lobby, player_index),
            display_name: format!("Player {}", player_index.display_number()),
        })
        .collect()
}

fn participant_identity(lobby: &Lobby, player_index: PlayerIndex) -> String {
    format!(
        "lobby-{}-p{}",
        lobby.lobby_id(),
        player_index.display_number()
    )
}

fn voice_state_from_response(
    response: CreateVoiceRoomBrokerResponse,
    mode: NetplayVoiceMode,
) -> RoomVoiceState {
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
        .filter_map(|grant| player_grant(grant, &shared, mode))
        .collect::<HashMap<_, _>>();

    RoomVoiceState::available(shared, grants)
}

fn player_grant(
    grant: VoiceBrokerGrant,
    shared: &RoomVoiceView,
    mode: NetplayVoiceMode,
) -> Option<(PlayerIndex, PlayerVoiceJoinGrant)> {
    let zero_based = grant.player_index.checked_sub(1)?;
    let player_index = PlayerIndex::new(zero_based, MAX_LOBBY_PLAYERS)?;

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
