//! Voice-chat state attached to a netplay room.
//!
//! Shared room views expose only provider metadata. Player-specific join tokens
//! stay in `PlayerVoiceJoinGrant`, which is returned only in `RoomJoin`.

use crate::protocol::NetplayVoiceMode;
use crate::rooms::PlayerIndex;
use serde::Serialize;
use std::collections::HashMap;

/// Shared voice-chat status exposed in `RoomView`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomVoiceStatus {
    /// Voice room was created and clients may use their private grants.
    Available,
    /// Voice was requested, but the broker was disabled or unavailable.
    Unavailable,
}

/// Shared voice-chat metadata safe to broadcast to every room subscriber.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomVoiceView {
    /// Current voice-room availability.
    pub status: RoomVoiceStatus,
    /// Voice provider backing this room.
    pub provider: Option<String>,
    /// ShadowBoy voice broker room id.
    pub voice_room_id: Option<String>,
    /// LiveKit room name.
    pub livekit_room_name: Option<String>,
    /// Public LiveKit WebSocket URL clients should connect to.
    pub server_url: Option<String>,
    /// Initial microphone behavior selected by the host.
    pub mode: NetplayVoiceMode,
    /// Maximum participants allowed in this voice room.
    pub max_participants: u8,
    /// Short user-safe status detail.
    pub status_detail: Option<String>,
}

/// Player-specific voice grant returned only to the matching joining socket.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerVoiceJoinGrant {
    /// Voice provider backing the token.
    pub provider: String,
    /// ShadowBoy voice broker room id.
    pub voice_room_id: String,
    /// LiveKit room name.
    pub livekit_room_name: String,
    /// Public LiveKit WebSocket URL.
    pub server_url: String,
    /// Stable LiveKit participant identity.
    pub participant_identity: String,
    /// Provider join token. This must never be placed in `RoomView`.
    pub token: String,
    /// RFC3339 token expiration timestamp.
    pub expires_at: String,
    /// Initial microphone behavior selected by the host.
    pub mode: NetplayVoiceMode,
}

/// Internal request details for a player-specific token refresh.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RoomVoiceTokenRefreshRequest {
    /// ShadowBoy voice broker room id.
    pub(crate) voice_room_id: String,
    /// One-based ShadowBoy player slot.
    pub(crate) player_index: PlayerIndex,
    /// Stable LiveKit identity.
    pub(crate) participant_identity: String,
    /// Display name sent to the voice broker.
    pub(crate) display_name: String,
}

/// Internal voice-room state with private per-player grants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RoomVoiceState {
    view: RoomVoiceView,
    grants: HashMap<PlayerIndex, PlayerVoiceJoinGrant>,
}

impl RoomVoiceState {
    /// Builds an available voice state from shared metadata and private grants.
    pub(crate) fn available(
        view: RoomVoiceView,
        grants: HashMap<PlayerIndex, PlayerVoiceJoinGrant>,
    ) -> Self {
        Self { view, grants }
    }

    /// Builds an unavailable state for rooms where voice was requested.
    pub(crate) fn unavailable(mode: NetplayVoiceMode, detail: impl Into<String>) -> Self {
        Self {
            view: RoomVoiceView {
                status: RoomVoiceStatus::Unavailable,
                provider: None,
                voice_room_id: None,
                livekit_room_name: None,
                server_url: None,
                mode,
                max_participants: 0,
                status_detail: Some(detail.into()),
            },
            grants: HashMap::new(),
        }
    }

    /// Returns shared metadata safe for room views and broadcasts.
    pub(crate) fn view(&self) -> RoomVoiceView {
        self.view.clone()
    }

    /// Returns the grant for one player, if the broker issued it.
    pub(crate) fn grant_for(&self, player_index: PlayerIndex) -> Option<PlayerVoiceJoinGrant> {
        self.grants.get(&player_index).cloned()
    }

    /// Refreshes one player's private token while preserving shared room data.
    pub(crate) fn refresh_grant(
        &mut self,
        player_index: PlayerIndex,
        participant_identity: String,
        token: String,
        expires_at: String,
    ) -> Option<PlayerVoiceJoinGrant> {
        let grant = self.grants.get_mut(&player_index)?;
        if grant.participant_identity != participant_identity {
            return None;
        }

        grant.token = token;
        grant.expires_at = expires_at;

        Some(grant.clone())
    }

    /// Returns the broker voice-room id for cleanup.
    pub(crate) fn voice_room_id(&self) -> Option<&str> {
        self.view.voice_room_id.as_deref()
    }
}
