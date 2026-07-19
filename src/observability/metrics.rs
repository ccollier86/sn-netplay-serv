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
    /// Records a successful reconnect.
    fn record_player_reconnected(&self);
    /// Records a session start broadcast.
    fn record_session_started(&self);
    /// Records a coordinated pause request.
    fn record_pause_requested(&self);
    /// Records a coordinated resume request.
    fn record_resume_requested(&self);
    /// Records a heartbeat accepted by the relay.
    fn record_heartbeat(&self);
    /// Records a stable protocol error.
    fn record_protocol_error(&self);
    /// Records one strict v5 input batch and its cursor outcome.
    fn record_v5_input_batch(
        &self,
        frame_count: u64,
        accepted_frames: u64,
        duplicate_frames: u64,
        nacked: bool,
    );
    /// Records one host frame-open request and whether it was idempotent.
    fn record_v5_host_frame_open(&self, duplicate: bool);
    /// Records one host-driven canonical frame release.
    fn record_v5_frame_released(&self);
    /// Records a scheduled start wake that had to wait for clock rounding.
    fn record_v5_scheduled_wake_retry(&self);
    /// Records an input socket closed after falling behind the event channel.
    fn record_input_event_lagged(&self);
    /// Records a protocol v5 input socket closed for excessive message rate.
    fn record_v5_input_rate_limited(&self);
    /// Records a rejected request due to rate limiting.
    fn record_rate_limited(&self);
    /// Records a license/auth failure.
    fn record_auth_rejected(&self);
    /// Records telemetry records dropped before reaching the durable queue.
    fn record_telemetry_dropped(&self, count: u64);
    /// Records telemetry records lost because the durable write failed.
    fn record_telemetry_write_failed(&self, count: u64);
    /// Returns a point-in-time metrics snapshot.
    fn snapshot(&self) -> MetricsSnapshot;
}

/// Atomic in-process metrics recorder.
#[derive(Default)]
pub struct InMemoryMetrics {
    rooms_created_total: AtomicU64,
    websocket_joins_total: AtomicU64,
    player_reconnects_total: AtomicU64,
    sessions_started_total: AtomicU64,
    pause_requests_total: AtomicU64,
    resume_requests_total: AtomicU64,
    heartbeats_total: AtomicU64,
    protocol_errors_total: AtomicU64,
    v5_input_batches_total: AtomicU64,
    v5_input_frames_total: AtomicU64,
    v5_input_frames_accepted_total: AtomicU64,
    v5_input_frames_duplicate_total: AtomicU64,
    v5_input_nacks_total: AtomicU64,
    v5_host_frame_opens_total: AtomicU64,
    v5_host_frame_open_duplicates_total: AtomicU64,
    v5_frame_releases_total: AtomicU64,
    v5_scheduled_wake_retries_total: AtomicU64,
    input_event_lagged_total: AtomicU64,
    v5_input_rate_limited_total: AtomicU64,
    rate_limited_total: AtomicU64,
    auth_rejected_total: AtomicU64,
    telemetry_dropped_total: AtomicU64,
    telemetry_write_failed_total: AtomicU64,
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

    fn record_player_reconnected(&self) {
        self.player_reconnects_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_session_started(&self) {
        self.sessions_started_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_pause_requested(&self) {
        self.pause_requests_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_resume_requested(&self) {
        self.resume_requests_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_heartbeat(&self) {
        self.heartbeats_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_protocol_error(&self) {
        self.protocol_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_v5_input_batch(
        &self,
        frame_count: u64,
        accepted_frames: u64,
        duplicate_frames: u64,
        nacked: bool,
    ) {
        self.v5_input_batches_total.fetch_add(1, Ordering::Relaxed);
        self.v5_input_frames_total
            .fetch_add(frame_count, Ordering::Relaxed);
        self.v5_input_frames_accepted_total
            .fetch_add(accepted_frames, Ordering::Relaxed);
        self.v5_input_frames_duplicate_total
            .fetch_add(duplicate_frames, Ordering::Relaxed);
        if nacked {
            self.v5_input_nacks_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_v5_host_frame_open(&self, duplicate: bool) {
        self.v5_host_frame_opens_total
            .fetch_add(1, Ordering::Relaxed);
        if duplicate {
            self.v5_host_frame_open_duplicates_total
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_v5_frame_released(&self) {
        self.v5_frame_releases_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_v5_scheduled_wake_retry(&self) {
        self.v5_scheduled_wake_retries_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_input_event_lagged(&self) {
        self.input_event_lagged_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_v5_input_rate_limited(&self) {
        self.v5_input_rate_limited_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_rate_limited(&self) {
        self.rate_limited_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_auth_rejected(&self) {
        self.auth_rejected_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_telemetry_dropped(&self, count: u64) {
        self.telemetry_dropped_total
            .fetch_add(count, Ordering::Relaxed);
    }

    fn record_telemetry_write_failed(&self, count: u64) {
        self.telemetry_write_failed_total
            .fetch_add(count, Ordering::Relaxed);
    }

    fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            rooms_created_total: self.rooms_created_total.load(Ordering::Relaxed),
            websocket_joins_total: self.websocket_joins_total.load(Ordering::Relaxed),
            player_reconnects_total: self.player_reconnects_total.load(Ordering::Relaxed),
            sessions_started_total: self.sessions_started_total.load(Ordering::Relaxed),
            pause_requests_total: self.pause_requests_total.load(Ordering::Relaxed),
            resume_requests_total: self.resume_requests_total.load(Ordering::Relaxed),
            heartbeats_total: self.heartbeats_total.load(Ordering::Relaxed),
            protocol_errors_total: self.protocol_errors_total.load(Ordering::Relaxed),
            v5_input_batches_total: self.v5_input_batches_total.load(Ordering::Relaxed),
            v5_input_frames_total: self.v5_input_frames_total.load(Ordering::Relaxed),
            v5_input_frames_accepted_total: self
                .v5_input_frames_accepted_total
                .load(Ordering::Relaxed),
            v5_input_frames_duplicate_total: self
                .v5_input_frames_duplicate_total
                .load(Ordering::Relaxed),
            v5_input_nacks_total: self.v5_input_nacks_total.load(Ordering::Relaxed),
            v5_host_frame_opens_total: self.v5_host_frame_opens_total.load(Ordering::Relaxed),
            v5_host_frame_open_duplicates_total: self
                .v5_host_frame_open_duplicates_total
                .load(Ordering::Relaxed),
            v5_frame_releases_total: self.v5_frame_releases_total.load(Ordering::Relaxed),
            v5_scheduled_wake_retries_total: self
                .v5_scheduled_wake_retries_total
                .load(Ordering::Relaxed),
            input_event_lagged_total: self.input_event_lagged_total.load(Ordering::Relaxed),
            v5_input_rate_limited_total: self.v5_input_rate_limited_total.load(Ordering::Relaxed),
            rate_limited_total: self.rate_limited_total.load(Ordering::Relaxed),
            auth_rejected_total: self.auth_rejected_total.load(Ordering::Relaxed),
            telemetry_dropped_total: self.telemetry_dropped_total.load(Ordering::Relaxed),
            telemetry_write_failed_total: self.telemetry_write_failed_total.load(Ordering::Relaxed),
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
    /// Total successful slot reconnects.
    pub player_reconnects_total: u64,
    /// Total sessions started.
    pub sessions_started_total: u64,
    /// Total coordinated pause requests.
    pub pause_requests_total: u64,
    /// Total coordinated resume requests.
    pub resume_requests_total: u64,
    /// Total accepted app-level heartbeats.
    pub heartbeats_total: u64,
    /// Total stable protocol errors.
    pub protocol_errors_total: u64,
    /// Total protocol v5 input batches received.
    pub v5_input_batches_total: u64,
    /// Total protocol v5 input frames received, including retransmits.
    pub v5_input_frames_total: u64,
    /// Total newly accepted protocol v5 input frames.
    pub v5_input_frames_accepted_total: u64,
    /// Total idempotent protocol v5 input retransmit frames.
    pub v5_input_frames_duplicate_total: u64,
    /// Total recoverable protocol v5 cursor NACKs.
    pub v5_input_nacks_total: u64,
    /// Total protocol v5 host frame-open requests.
    pub v5_host_frame_opens_total: u64,
    /// Total idempotent duplicate host frame-open requests.
    pub v5_host_frame_open_duplicates_total: u64,
    /// Total protocol v5 canonical frame releases.
    pub v5_frame_releases_total: u64,
    /// Total one-shot start wakes retried for clock rounding.
    pub v5_scheduled_wake_retries_total: u64,
    /// Total input sockets closed because the bounded event stream lagged.
    pub input_event_lagged_total: u64,
    /// Total protocol v5 input sockets closed for excessive message rate.
    pub v5_input_rate_limited_total: u64,
    /// Total HTTP requests rejected by rate limits.
    pub rate_limited_total: u64,
    /// Total license/auth failures.
    pub auth_rejected_total: u64,
    /// Total telemetry records dropped because the nonblocking queue was full or closed.
    pub telemetry_dropped_total: u64,
    /// Total telemetry records discarded after a durable write failure.
    pub telemetry_write_failed_total: u64,
}

#[cfg(test)]
mod tests {
    use super::{InMemoryMetrics, MetricsRecorder};

    #[test]
    fn snapshot_returns_recorded_counts() {
        let metrics = InMemoryMetrics::new();

        metrics.record_room_created();
        metrics.record_websocket_joined();
        metrics.record_v5_input_batch(4, 2, 2, true);
        metrics.record_v5_host_frame_open(false);
        metrics.record_v5_host_frame_open(true);
        metrics.record_v5_frame_released();
        metrics.record_v5_scheduled_wake_retry();
        metrics.record_input_event_lagged();
        metrics.record_v5_input_rate_limited();
        metrics.record_rate_limited();
        metrics.record_auth_rejected();
        metrics.record_telemetry_dropped(2);
        metrics.record_telemetry_write_failed(3);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.rooms_created_total, 1);
        assert_eq!(snapshot.websocket_joins_total, 1);
        assert_eq!(snapshot.v5_input_batches_total, 1);
        assert_eq!(snapshot.v5_input_frames_total, 4);
        assert_eq!(snapshot.v5_input_frames_accepted_total, 2);
        assert_eq!(snapshot.v5_input_frames_duplicate_total, 2);
        assert_eq!(snapshot.v5_input_nacks_total, 1);
        assert_eq!(snapshot.v5_host_frame_opens_total, 2);
        assert_eq!(snapshot.v5_host_frame_open_duplicates_total, 1);
        assert_eq!(snapshot.v5_frame_releases_total, 1);
        assert_eq!(snapshot.v5_scheduled_wake_retries_total, 1);
        assert_eq!(snapshot.input_event_lagged_total, 1);
        assert_eq!(snapshot.v5_input_rate_limited_total, 1);
        assert_eq!(snapshot.rate_limited_total, 1);
        assert_eq!(snapshot.auth_rejected_total, 1);
        assert_eq!(snapshot.telemetry_dropped_total, 2);
        assert_eq!(snapshot.telemetry_write_failed_total, 3);
    }
}
