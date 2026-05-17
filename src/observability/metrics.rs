//! Low-overhead process metrics.
//!
//! Counters are atomic and intentionally coarse. They provide enough visibility
//! for launch without adding an external metrics backend dependency.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics sink used by route handlers.
pub trait MetricsRecorder: Send + Sync {
    /// Records a successfully created room.
    fn record_room_created(&self);
    /// Records a successfully upgraded WebSocket join.
    fn record_websocket_joined(&self);
    /// Records a rejected request due to rate limiting.
    fn record_rate_limited(&self);
    /// Records a license/auth failure.
    fn record_auth_rejected(&self);
    /// Returns a point-in-time metrics snapshot.
    fn snapshot(&self) -> MetricsSnapshot;
}

/// Atomic in-process metrics recorder.
#[derive(Default)]
pub struct InMemoryMetrics {
    rooms_created_total: AtomicU64,
    websocket_joins_total: AtomicU64,
    rate_limited_total: AtomicU64,
    auth_rejected_total: AtomicU64,
}

impl InMemoryMetrics {
    /// Creates an empty metrics recorder.
    pub fn new() -> Self {
        Self::default()
    }
}

impl MetricsRecorder for InMemoryMetrics {
    fn record_room_created(&self) {
        self.rooms_created_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_websocket_joined(&self) {
        self.websocket_joins_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_rate_limited(&self) {
        self.rate_limited_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_auth_rejected(&self) {
        self.auth_rejected_total.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            rooms_created_total: self.rooms_created_total.load(Ordering::Relaxed),
            websocket_joins_total: self.websocket_joins_total.load(Ordering::Relaxed),
            rate_limited_total: self.rate_limited_total.load(Ordering::Relaxed),
            auth_rejected_total: self.auth_rejected_total.load(Ordering::Relaxed),
        }
    }
}

/// Serializable metrics response.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSnapshot {
    /// Total successful room creations.
    pub rooms_created_total: u64,
    /// Total successful WebSocket joins.
    pub websocket_joins_total: u64,
    /// Total HTTP requests rejected by rate limits.
    pub rate_limited_total: u64,
    /// Total license/auth failures.
    pub auth_rejected_total: u64,
}

#[cfg(test)]
mod tests {
    use super::{InMemoryMetrics, MetricsRecorder};

    #[test]
    fn snapshot_returns_recorded_counts() {
        let metrics = InMemoryMetrics::new();

        metrics.record_room_created();
        metrics.record_websocket_joined();
        metrics.record_rate_limited();
        metrics.record_auth_rejected();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.rooms_created_total, 1);
        assert_eq!(snapshot.websocket_joins_total, 1);
        assert_eq!(snapshot.rate_limited_total, 1);
        assert_eq!(snapshot.auth_rejected_total, 1);
    }
}
