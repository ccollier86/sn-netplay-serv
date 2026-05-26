//! Voice-chat helpers for persistent lobbies.
//!
//! Lobby voice persists across child game launches, so this module keeps voice
//! token state attached to the lobby instead of active gameplay rooms.

use crate::lobbies::{Lobby, LobbyError};
use crate::rooms::{
    ConnectionId, PlayerIndex, PlayerVoiceJoinGrant, RoomVoiceState, RoomVoiceTokenRefreshRequest,
};

impl Lobby {
    /// Attaches broker voice state to this lobby.
    pub(crate) fn set_voice_state(&mut self, voice: RoomVoiceState) {
        self.voice = Some(voice);
    }

    /// Returns the private voice grant for one lobby player.
    pub(crate) fn voice_grant_for(
        &self,
        player_index: PlayerIndex,
    ) -> Option<PlayerVoiceJoinGrant> {
        self.voice
            .as_ref()
            .and_then(|voice| voice.grant_for(player_index))
    }

    /// Builds a broker request for refreshing this connection's voice token.
    pub(crate) fn voice_token_refresh_request(
        &self,
        connection_id: ConnectionId,
    ) -> Result<RoomVoiceTokenRefreshRequest, LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        let grant = self
            .voice_grant_for(player_index)
            .ok_or(LobbyError::VoiceUnavailable)?;
        let display_name = self
            .slot(player_index)
            .and_then(|slot| slot.display_name.clone())
            .unwrap_or_else(|| format!("Player {}", player_index.display_number()));

        Ok(RoomVoiceTokenRefreshRequest {
            voice_room_id: grant.voice_room_id,
            player_index,
            participant_identity: grant.participant_identity,
            display_name,
        })
    }

    /// Stores a freshly issued voice token for this connection's participant.
    pub(crate) fn refresh_voice_grant(
        &mut self,
        connection_id: ConnectionId,
        participant_identity: String,
        token: String,
        expires_at: String,
    ) -> Result<PlayerVoiceJoinGrant, LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        self.voice
            .as_mut()
            .and_then(|voice| {
                voice.refresh_grant(player_index, participant_identity, token, expires_at)
            })
            .ok_or(LobbyError::VoiceUnavailable)
    }

    /// Removes voice state and returns the broker room id for one-time cleanup.
    pub(crate) fn take_voice_room_id_for_cleanup(&mut self) -> Option<String> {
        let voice_room_id = self
            .voice
            .as_ref()
            .and_then(|voice| voice.voice_room_id().map(ToOwned::to_owned));
        if voice_room_id.is_some() {
            self.voice = None;
        }
        voice_room_id
    }
}
