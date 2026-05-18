//! Sanitized room event logs for operator debugging.
//!
//! Debug events intentionally contain room lifecycle facts only. They must not
//! include access tokens, resume tokens, raw input bytes, snapshot bytes, or
//! other client secrets.

use crate::rooms::RoomId;
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_ROOM_EVENT_CAPACITY: usize = 200;

/// Sanitized event emitted by room lifecycle and protocol operations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomDebugEvent {
    /// Milliseconds since unix epoch when the event was recorded.
    pub timestamp_ms: u128,
    /// Stable room id for correlation.
    pub room_id: RoomId,
    /// Optional invite code display value.
    pub invite_code: String,
    /// Monotonic room event sequence after the event is applied.
    pub event_seq: u64,
    /// Current room epoch.
    pub room_epoch: u64,
    /// Current session epoch.
    pub session_epoch: u64,
    /// Stable event kind.
    pub kind: String,
    /// Short human-safe summary.
    pub detail: String,
}

/// Fixed-size ring buffer for sanitized room debug events.
#[derive(Clone, Debug)]
pub struct RoomDebugEventLog {
    capacity: usize,
    events: VecDeque<RoomDebugEvent>,
}

impl Default for RoomDebugEventLog {
    fn default() -> Self {
        Self::new(DEFAULT_ROOM_EVENT_CAPACITY)
    }
}

impl RoomDebugEventLog {
    /// Creates an empty log with a bounded capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            events: VecDeque::with_capacity(capacity),
        }
    }

    /// Appends one sanitized event, evicting the oldest event if needed.
    pub fn push(&mut self, event: RoomDebugEvent) {
        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }

        self.events.push_back(event);
    }

    /// Returns the newest events in chronological order.
    pub fn tail(&self, limit: usize) -> Vec<RoomDebugEvent> {
        let skip = self.events.len().saturating_sub(limit);

        self.events.iter().skip(skip).cloned().collect()
    }
}

/// Returns a timestamp suitable for sanitized debug responses.
pub fn current_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}
