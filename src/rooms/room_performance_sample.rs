//! Sanitized per-player performance samples for durable telemetry.

use crate::protocol::{ClientNetworkQualityReport, ClientRuntimeState};
use crate::rooms::RoomId;

/// One client heartbeat/runtime sample.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomPerformanceSample {
    /// Milliseconds since unix epoch when the sample was recorded.
    pub timestamp_ms: u128,
    /// Stable room id for correlation.
    pub room_id: RoomId,
    /// Optional invite code display value.
    pub invite_code: String,
    /// Current room event sequence.
    pub event_seq: u64,
    /// Current room epoch.
    pub room_epoch: u64,
    /// Current session epoch.
    pub session_epoch: u64,
    /// Zero-based player index.
    pub player_index: u8,
    /// Client-reported runtime state.
    pub runtime_state: ClientRuntimeState,
    /// Client-reported deterministic emulation frame.
    pub local_frame: Option<u64>,
    /// Latest relay canonical frame.
    pub canonical_frame: u64,
    /// Latest released input socket frame.
    pub released_frame: Option<u64>,
    /// Next relay frame to release.
    pub next_release_frame: u64,
    /// Latest accepted input frame for this player.
    pub accepted_input_frame: Option<u64>,
    /// `local_frame - canonical_frame`, if local frame was reported.
    pub frame_delta: Option<i64>,
    /// Client-reported network/runtime sample.
    pub network: Option<ClientNetworkQualityReport>,
}
