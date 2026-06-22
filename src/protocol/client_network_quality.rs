//! Client-reported network and runtime health samples.
//!
//! The relay owns input-delay decisions, but clients provide the local samples
//! the relay cannot observe directly: RTT, jitter, and runtime pressure.

use serde::{Deserialize, Serialize};

/// Optional health sample included with heartbeat and ready messages.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientNetworkQualityReport {
    /// Last measured control-socket round trip in milliseconds.
    #[serde(default)]
    pub round_trip_ms: Option<u32>,
    /// Jitter estimate in milliseconds, preferably smoothed by the client.
    #[serde(default)]
    pub jitter_ms: Option<u32>,
    /// Frames the local runtime is predicting ahead of confirmed relay frames.
    #[serde(default)]
    pub prediction_frames: Option<u32>,
    /// Local stall count since the previous health report.
    #[serde(default)]
    pub stall_count: Option<u32>,
    /// Catch-up frame count since the previous health report.
    #[serde(default)]
    pub catch_up_frames: Option<u32>,
    /// Late-input corrections observed since the previous health report.
    #[serde(default)]
    pub late_input_frames: Option<u32>,
    /// Audio underruns observed since the previous health report.
    #[serde(default)]
    pub audio_underruns: Option<u32>,
    /// Latest clock-sync uncertainty estimate in milliseconds.
    #[serde(default)]
    pub clock_uncertainty_ms: Option<u32>,
}
