//! Room lifecycle status values.
//!
//! Status is serialized to clients and also drives allowed room operations. It
//! stays separate from the room state machine so protocol additions do not
//! bloat the domain model file.

use serde::Serialize;

/// Lifecycle status for a netplay room.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomStatus {
    /// Host is waiting for a guest to join.
    WaitingForGuest,
    /// Clients are comparing compatibility fingerprints.
    CheckingCompatibility,
    /// Clients are syncing host state before starting.
    SyncingState,
    /// Clients are ready for snapshot sync or start.
    Ready,
    /// Gameplay input relay is active.
    Playing,
    /// Gameplay is paused by the coordinated netplay pause contract.
    Paused,
    /// Room is no longer accepting traffic.
    Closed,
}
