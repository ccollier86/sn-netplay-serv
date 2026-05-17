//! Serializable active-room registry snapshots.
//!
//! Snapshots power internal observability endpoints without exposing private
//! license tokens or transport internals.

use crate::rooms::RoomView;
use serde::Serialize;

/// Point-in-time view of active room storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRegistrySnapshot {
    /// Number of active rooms held by this server process.
    pub active_room_count: usize,
    /// Current room views for debugging and admin dashboards.
    pub rooms: Vec<RoomView>,
}
