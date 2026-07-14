//! Host-authorized lobby player removal models.

use crate::rooms::{ConnectionId, PlayerIndex};
use serde::Serialize;

/// Stable reason delivered to a removed lobby participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyPlayerRemovalReason {
    /// The lobby host removed this participant.
    RemovedByHost,
}

/// Private state captured before an occupied guest slot is erased.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LobbyPlayerRemoval {
    /// Removed zero-based player slot.
    pub(crate) player_index: PlayerIndex,
    /// Active socket to terminate, when the player was connected.
    pub(crate) connection_id: Option<ConnectionId>,
    /// Voice broker room owning the participant session, when voice is active.
    pub(crate) voice_room_id: Option<String>,
    /// Provider participant identity to disconnect, when voice is active.
    pub(crate) participant_identity: Option<String>,
}
