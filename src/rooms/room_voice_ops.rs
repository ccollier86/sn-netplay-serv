//! Voice-chat helpers for active netplay rooms.
//!
//! Keeping these helpers outside `room.rs` prevents optional voice setup from
//! diluting the core room lifecycle state machine.

use crate::protocol::NetplayVoiceMode;
use crate::rooms::{NetplayRoom, PlayerIndex, PlayerVoiceJoinGrant, RoomVoiceState};

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
