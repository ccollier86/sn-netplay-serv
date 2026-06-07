//! Background cleanup for abandoned persistent lobbies.
//!
//! Lobby cleanup is intentionally separate from gameplay-room cleanup because
//! lobbies are allowed to survive short disconnects and breaks.

use crate::lobbies::InMemoryLobbyRegistry;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::debug;

const LOBBY_EXPIRATION_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// Starts the periodic idle-lobby cleanup task.
pub fn spawn_lobby_expiration_task(
    registry: Arc<InMemoryLobbyRegistry>,
    idle_timeout: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(LOBBY_EXPIRATION_SWEEP_INTERVAL);

        loop {
            interval.tick().await;
            let expired_count = registry.expire_idle_lobbies(idle_timeout).await;

            if expired_count > 0 {
                debug!(expired_count, "expired idle netplay lobbies");
            }
        }
    })
}
