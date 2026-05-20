//! Background controller-netplay frame clock.
//!
//! The registry accepts bounded future input, but this task releases canonical
//! frames one at a time. That mirrors the server-clock role used by established
//! rollback netplay instead of flushing every future frame as soon as both
//! clients submit it.

use crate::limits::ROOM_FRAME_CLOCK_INTERVAL;
use crate::rooms::InMemoryRoomRegistry;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Starts the periodic controller frame release task.
pub fn spawn_room_frame_clock_task(registry: Arc<InMemoryRoomRegistry>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(ROOM_FRAME_CLOCK_INTERVAL);

        loop {
            interval.tick().await;
            registry.release_next_controller_frames().await;
        }
    })
}
