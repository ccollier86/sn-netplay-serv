//! Serializable lobby views.
//!
//! Views expose enough state for UI and SDK state machines without leaking raw
//! resume tokens or internal authenticated subject keys.

use crate::lobbies::{
    LobbyGameLaunchView, LobbyGameReadinessView, LobbyGameSelectionView, LobbyPlayerSlotView,
    LobbyServerCapabilities, LobbyStatus,
};
use crate::rooms::{RoomId, RoomVoiceView};
use serde::Serialize;

/// Current lobby state returned by REST and future lobby WebSocket messages.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyView {
    /// Stable lobby id.
    pub lobby_id: RoomId,
    /// Monotonic event sequence for lobby state changes.
    pub event_seq: u64,
    /// Epoch that changes when lobby membership or selected game changes.
    pub lobby_epoch: u64,
    /// User-facing invite code.
    pub invite_code: String,
    /// Creation timestamp in milliseconds since unix epoch.
    pub created_at_ms: u128,
    /// Last state-change timestamp in milliseconds since unix epoch.
    pub updated_at_ms: u128,
    /// Current lobby lifecycle status.
    pub status: LobbyStatus,
    /// Server capability flags for this lobby.
    pub capabilities: LobbyServerCapabilities,
    /// Current player slots in display order.
    pub players: Vec<LobbyPlayerSlotView>,
    /// Selected game proposal, if one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_game: Option<LobbyGameSelectionView>,
    /// Player readiness for the selected game.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub game_readiness: Vec<LobbyGameReadinessView>,
    /// Host launch signal for the selected game.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_launch: Option<LobbyGameLaunchView>,
    /// Lobby-scoped voice room metadata safe to broadcast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<RoomVoiceView>,
}
