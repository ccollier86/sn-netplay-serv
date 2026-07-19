//! Tunable room recovery timing.
//!
//! The relay keeps recovery state in memory for the current single-instance
//! deployment. These durations are parsed once from environment configuration
//! and injected into the registry.

use std::time::Duration;

/// Timing policy for reconnect, heartbeat, and idle cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomRecoveryConfig {
    /// How long a delivered runner handoff capability may wait for the runner.
    pub runner_handoff_grace: Duration,
    /// How long a disconnected player may reclaim the same slot.
    pub reconnect_grace: Duration,
    /// How long without heartbeat before a player is considered stale.
    pub heartbeat_stale: Duration,
    /// How long without heartbeat before a player is moved into recovery.
    pub heartbeat_disconnect: Duration,
    /// How long a completely idle room may remain in memory.
    pub room_idle: Duration,
}

impl Default for RoomRecoveryConfig {
    fn default() -> Self {
        Self {
            runner_handoff_grace: Duration::from_secs(60),
            reconnect_grace: Duration::from_secs(90),
            heartbeat_stale: Duration::from_secs(15),
            heartbeat_disconnect: Duration::from_secs(30),
            room_idle: Duration::from_secs(300),
        }
    }
}
