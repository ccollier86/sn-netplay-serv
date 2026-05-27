//! Sanitized lobby event logs for operator debugging.
//!
//! Lobby diagnostics must not include access tokens, resume tokens, chat bodies,
//! ROM metadata beyond proposal summaries, or license secrets.

use crate::lobbies::LobbyView;
use crate::rooms::RoomId;
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_LOBBY_EVENT_CAPACITY: usize = 200;

/// Sanitized event emitted by lobby lifecycle and protocol operations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyDebugEvent {
    /// Milliseconds since unix epoch when the event was recorded.
    pub timestamp_ms: u128,
    /// Stable lobby id for correlation.
    pub lobby_id: RoomId,
    /// Human invite code for operator correlation.
    pub invite_code: String,
    /// Monotonic lobby event sequence after the event is applied.
    pub event_seq: u64,
    /// Current lobby epoch.
    pub lobby_epoch: u64,
    /// Stable event kind.
    pub kind: String,
    /// Short human-safe summary.
    pub detail: String,
}

/// Current active lobby views for authenticated operators.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyRegistrySnapshot {
    /// Number of lobbies retained by this relay process.
    pub active_lobby_count: usize,
    /// Current lobby views.
    pub lobbies: Vec<LobbyView>,
}

/// Fixed-size ring buffer for sanitized lobby debug events.
#[derive(Clone, Debug)]
pub struct LobbyDebugEventLog {
    capacity: usize,
    events: VecDeque<LobbyDebugEvent>,
}

/// Nonblocking sink for durable lobby-event telemetry.
pub trait LobbyDebugEventSink: Send + Sync {
    /// Records one sanitized event without waiting on external storage.
    fn record_lobby_event(&self, event: LobbyDebugEvent);
}

/// Sink used when durable telemetry is disabled.
#[derive(Debug, Default)]
pub struct NoopLobbyDebugEventSink;

impl LobbyDebugEventSink for NoopLobbyDebugEventSink {
    fn record_lobby_event(&self, _event: LobbyDebugEvent) {}
}

impl Default for LobbyDebugEventLog {
    fn default() -> Self {
        Self::new(DEFAULT_LOBBY_EVENT_CAPACITY)
    }
}

impl LobbyDebugEventLog {
    /// Creates an empty log with a bounded capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            events: VecDeque::with_capacity(capacity),
        }
    }

    /// Appends one sanitized event, evicting the oldest event if needed.
    pub fn push(&mut self, event: LobbyDebugEvent) {
        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }

        self.events.push_back(event);
    }

    /// Returns the newest events in chronological order.
    pub fn tail(&self, limit: usize) -> Vec<LobbyDebugEvent> {
        let skip = self.events.len().saturating_sub(limit);

        self.events.iter().skip(skip).cloned().collect()
    }
}

/// Returns a timestamp suitable for sanitized debug responses.
pub fn current_lobby_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}
