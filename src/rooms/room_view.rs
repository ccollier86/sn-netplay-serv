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
    /// Monotonic room event sequence.
    pub event_seq: u64,
    /// Epoch that changes when room membership or recovery state changes.
    pub room_epoch: u64,
    /// Epoch that changes when active gameplay must resync.
    pub session_epoch: u64,
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
    /// Explicit runtime state used by clients for recovery and UI.
    pub runtime_state: crate::rooms::PlayerRuntimeState,
    /// Whether a verified player currently occupies the slot.
    pub occupied: bool,
    /// Milliseconds since this slot was last seen by heartbeat or socket IO.
    pub last_seen_age_ms: Option<u128>,
    /// Milliseconds remaining before reconnect grace expires.
    pub reconnect_grace_remaining_ms: Option<u128>,
}
