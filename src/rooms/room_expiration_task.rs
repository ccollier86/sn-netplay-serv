//! Background cleanup for abandoned in-memory rooms.
//!
//! This module owns the periodic timer that asks the registry to remove rooms
//! waiting too long for a guest. It does not inspect socket state directly.

use crate::limits::{ROOM_EXPIRATION_SWEEP_INTERVAL, ROOM_JOIN_TIMEOUT};
use crate::rooms::InMemoryRoomRegistry;
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinHandle;
use tracing::debug;

/// Starts the periodic abandoned-room cleanup task.
pub fn spawn_room_expiration_task(registry: Arc<InMemoryRoomRegistry>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(ROOM_EXPIRATION_SWEEP_INTERVAL);

        loop {
            interval.tick().await;
            let expired_count = registry
                .remove_expired_waiting_rooms(Instant::now(), ROOM_JOIN_TIMEOUT)
                .await;

            if expired_count > 0 {
                debug!(expired_count, "expired abandoned netplay rooms");
            }
        }
    })
}
