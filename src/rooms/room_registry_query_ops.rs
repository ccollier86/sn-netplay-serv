//! Query and debug helpers for the in-memory room registry.
//!
//! This module owns read-only room snapshots and sanitized event-log reads used
//! by public status and internal operator endpoints.

use super::InMemoryRoomRegistry;
use crate::rooms::{InviteCode, RoomDebugEvent, RoomError, RoomRegistrySnapshot, RoomView};

impl InMemoryRoomRegistry {
    /// Returns a serializable view for one invite code.
    pub(super) async fn room_view_impl(
        &self,
        invite_code: InviteCode,
    ) -> Result<RoomView, RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        Ok(stored_room.view(self.clock.now()))
    }

    /// Returns recent sanitized events for one active room.
    pub(super) async fn room_events_impl(
        &self,
        invite_code: InviteCode,
        limit: usize,
    ) -> Result<Vec<RoomDebugEvent>, RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        Ok(stored_room.debug_events(limit))
    }

    /// Returns recent sanitized events across active rooms.
    pub(super) async fn recent_events_impl(&self, limit: usize) -> Vec<RoomDebugEvent> {
        self.recent_events
            .lock()
            .map(|events| events.tail(limit))
            .unwrap_or_default()
    }

    /// Returns a point-in-time snapshot of active rooms.
    pub(super) async fn snapshot_impl(&self) -> RoomRegistrySnapshot {
        let rooms = self.invite_codes.read().await;
        let now = self.clock.now();
        let views = rooms
            .values()
            .map(|stored_room| stored_room.view(now))
            .collect::<Vec<_>>();

        RoomRegistrySnapshot {
            active_room_count: views.len(),
            rooms: views,
        }
    }
}
