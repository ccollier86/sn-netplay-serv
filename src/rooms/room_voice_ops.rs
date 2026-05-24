//! Voice-chat helpers for active netplay rooms.
//!
//! Keeping these helpers outside `room.rs` prevents optional voice setup from
//! diluting the core room lifecycle state machine.

use crate::protocol::NetplayVoiceMode;
use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerIndex, PlayerVoiceJoinGrant, RoomError, RoomVoiceState,
    RoomVoiceTokenRefreshRequest,
};

impl NetplayRoom {
    /// Returns whether this room requested voice chat at creation time.
    pub(crate) fn voice_requested(&self) -> bool {
        self.session
            .voice
            .as_ref()
            .is_some_and(|voice| voice.enabled)
    }

    /// Returns the requested voice mode, defaulting to voice activation.
    pub(crate) fn requested_voice_mode(&self) -> NetplayVoiceMode {
        self.session
            .voice
            .as_ref()
            .map(|voice| voice.mode)
            .unwrap_or_default()
    }

    /// Attaches broker voice state to this room.
    pub(crate) fn set_voice_state(&mut self, voice: RoomVoiceState) {
        self.voice = Some(voice);
    }

    /// Returns the private voice grant for the requested player.
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
    ) -> Result<RoomVoiceTokenRefreshRequest, RoomError> {
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let grant = self
            .voice_grant_for(player_index)
            .ok_or(RoomError::VoiceUnavailable)?;

        Ok(RoomVoiceTokenRefreshRequest {
            voice_room_id: grant.voice_room_id,
            player_index,
            participant_identity: grant.participant_identity,
            display_name: format!("Player {}", player_index.display_number()),
        })
    }

    /// Stores a freshly issued token for this connection's voice participant.
    pub(crate) fn refresh_voice_grant(
        &mut self,
        connection_id: ConnectionId,
        participant_identity: String,
        token: String,
        expires_at: String,
    ) -> Result<PlayerVoiceJoinGrant, RoomError> {
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        self.voice
            .as_mut()
            .and_then(|voice| {
                voice.refresh_grant(player_index, participant_identity, token, expires_at)
            })
            .ok_or(RoomError::VoiceUnavailable)
    }

    /// Returns the voice broker room id that should be cleaned up.
    pub(crate) fn voice_room_id_for_cleanup(&self) -> Option<String> {
        self.voice
            .as_ref()
            .and_then(|voice| voice.voice_room_id().map(ToOwned::to_owned))
    }

    /// Removes voice state and returns the broker room id for one-time cleanup.
    pub(crate) fn take_voice_room_id_for_cleanup(&mut self) -> Option<String> {
        let voice_room_id = self.voice_room_id_for_cleanup();
        if voice_room_id.is_some() {
            self.voice = None;
        }
        voice_room_id
    }
}
