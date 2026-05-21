//! Relay-owned adaptive input-delay update messages.
//!
//! Delay changes are scheduled for a future canonical frame so every client can
//! apply the new value at the same deterministic point in the input timeline.

use serde::Serialize;

/// Reason the relay changed controller-netplay input delay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InputDelayChangeReason {
    /// Initial startup value selected from pre-game latency samples.
    InitialLatency,
    /// Runtime health showed late input, high RTT, or prediction pressure.
    NetworkPressure,
    /// Runtime health stayed stable long enough to reduce delay.
    StableConnection,
}

/// Scheduled input-delay change shared with clients.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDelayChange {
    /// Canonical frame where clients apply the new input delay.
    pub effective_frame: u64,
    /// New input-delay frame count selected by the relay.
    pub input_delay_frames: u8,
    /// Previous input-delay frame count.
    pub previous_input_delay_frames: u8,
    /// Relay decision reason for diagnostics and client logs.
    pub reason: InputDelayChangeReason,
}
