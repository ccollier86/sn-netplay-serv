//! Serializable room views returned by HTTP and WebSocket responses.
//!
//! These DTOs stay separate from room mutation logic so adding UI-facing fields
//! does not bloat the domain state machine.

use crate::protocol::{NetplayProtocolView, NetplaySessionDescriptor, SessionPauseView};
use crate::rooms::{PlayerRole, PlayerStatus, RoomId, RoomStatus};
use serde::Serialize;

/// Serializable room state view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomView {
    /// Stable room id.
    pub room_id: RoomId,
    /// User-facing invite code.
    pub invite_code: String,
    /// Relay protocol metadata.
    pub protocol: NetplayProtocolView,
    /// Game/core session descriptor used for local ROM matching.
    pub session: NetplaySessionDescriptor,
    /// Configured room capacity.
    pub max_players: u8,
    /// Active coordinated pause details, if any.
    pub pause: Option<SessionPauseView>,
    /// Current room lifecycle status.
    pub status: RoomStatus,
    /// Player slots in display order.
    pub players: Vec<PlayerSlotView>,
}

/// Serializable player slot view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSlotView {
    /// Zero-based protocol player index.
    pub player_index: u8,
    /// One-based player number for UI display.
    pub display_number: u8,
    /// Server-assigned role.
    pub role: PlayerRole,
    /// User-facing slot status.
    pub status: PlayerStatus,
    /// Whether a verified player currently occupies the slot.
    pub occupied: bool,
}
