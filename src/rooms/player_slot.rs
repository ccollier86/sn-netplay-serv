//! Player slot state for active rooms.
//!
//! Slots are capacity-based instead of host/guest fields so the protocol can
//! expand past two players later without changing its shape.

use crate::auth::VerifiedLicense;
use crate::rooms::{ConnectionId, PlayerIndex};
use serde::Serialize;

/// Role assigned by the server when a player joins.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerRole {
    /// Room creator and Player 1 for the MVP.
    Host,
    /// Joined player.
    Guest,
}

/// User-facing player status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerStatus {
    /// No player occupies this slot.
    Empty,
    /// Player is connected to the room.
    Connected,
    /// Player is comparing compatibility fingerprints.
    CheckingCompatibility,
    /// Player failed compatibility checks.
    CompatibilityFailed,
    /// Player is receiving or sending sync state.
    SyncingState,
    /// Player is ready to start.
    Ready,
    /// Player is in active gameplay.
    Playing,
    /// Player is paused by coordinated netplay pause.
    Paused,
    /// Player disconnected.
    Disconnected,
}

/// Slot assigned to one player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSlot {
    /// Zero-based player index used by protocol messages.
    pub player_index: PlayerIndex,
    /// Role assigned by the server.
    pub role: PlayerRole,
    /// Verified client identity occupying the slot.
    pub subject_key: Option<String>,
    /// Active socket connection occupying the slot.
    pub connection_id: Option<ConnectionId>,
    /// Optional name shown in Desktop room UI.
    pub display_name: Option<String>,
    /// Current lifecycle status.
    pub status: PlayerStatus,
}

impl PlayerSlot {
    /// Creates an empty slot for `player_index`.
    pub fn empty(player_index: PlayerIndex) -> Self {
        Self {
            player_index,
            role: PlayerRole::Guest,
            subject_key: None,
            connection_id: None,
            display_name: None,
            status: PlayerStatus::Empty,
        }
    }

    /// Creates the host slot from a verified license.
    pub fn host(license: &VerifiedLicense, connection_id: ConnectionId) -> Self {
        Self {
            player_index: PlayerIndex::ONE,
            role: PlayerRole::Host,
            subject_key: Some(license.identity_key()),
            connection_id: Some(connection_id),
            display_name: None,
            status: PlayerStatus::Connected,
        }
    }

    /// Marks an empty slot as occupied by a guest.
    pub fn occupy_guest(&mut self, license: &VerifiedLicense, connection_id: ConnectionId) {
        self.role = PlayerRole::Guest;
        self.subject_key = Some(license.identity_key());
        self.connection_id = Some(connection_id);
        self.status = PlayerStatus::Connected;
    }

    /// Returns whether the slot is available.
    pub fn is_empty(&self) -> bool {
        self.status == PlayerStatus::Empty
    }
}
