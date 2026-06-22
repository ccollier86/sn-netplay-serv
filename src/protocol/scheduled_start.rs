//! Scheduled-start protocol values for synchronized netplay release.
//!
//! These DTOs describe when clients may release frame zero. They do not own
//! clock sampling, room storage, or runner launch behavior.

use crate::protocol::ClockSyncEstimate;
use serde::{Deserialize, Serialize};

/// Server-authored synchronized gameplay release contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledSessionStart {
    /// Room epoch the start belongs to.
    pub room_epoch: u64,
    /// Session epoch the start belongs to.
    pub session_epoch: u64,
    /// Canonical frame released at the scheduled time.
    pub start_frame: u64,
    /// Future server monotonic timestamp in milliseconds.
    pub server_time_ms: u64,
    /// Server monotonic timestamp when this contract was created.
    pub created_at_server_time_ms: u64,
    /// Minimum delay floor used when scheduling this start.
    pub minimum_start_delay_ms: u64,
    /// Clock/network uncertainty budget included in the selected delay.
    pub clock_uncertainty_budget_ms: u64,
}

/// Client report that its local runner is ready for deterministic release.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeterministicReadyReport {
    /// Client monotonic time when runner readiness was reached.
    pub local_ready_time_ms: u64,
    /// Frames run before declaring deterministic readiness.
    pub warmup_frame_count: u64,
    /// State frame loaded during startup, when startup restore was used.
    #[serde(default)]
    pub loaded_state_frame: Option<u64>,
    /// Optional client-computed clock estimate for diagnostics.
    #[serde(default)]
    pub clock: Option<ClockSyncEstimate>,
}
