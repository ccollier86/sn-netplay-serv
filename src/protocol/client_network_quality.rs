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
    /// Input frames resent since the previous health report.
    #[serde(default)]
    pub input_resend_frames: Option<u32>,
    /// Input NACKs received since the previous health report.
    #[serde(default)]
    pub input_nacks: Option<u32>,
    /// Emulator frames replayed since the previous health report.
    #[serde(default)]
    pub replayed_frames: Option<u32>,
    /// Replay audio frames intentionally suppressed since the previous report.
    #[serde(default)]
    pub suppressed_audio_frames: Option<u32>,
    /// Replay video frames intentionally suppressed since the previous report.
    #[serde(default)]
    pub suppressed_video_frames: Option<u32>,
    /// Current queued audio depth in emulated frames.
    #[serde(default)]
    pub audio_queue_depth_frames: Option<u32>,
    /// Audio catch-up operations since the previous health report.
    #[serde(default)]
    pub audio_catch_up_events: Option<u32>,
    /// Audio frames trimmed since the previous health report.
    #[serde(default)]
    pub audio_trimmed_frames: Option<u32>,
    /// Sustained audio recovery-prefill events since the previous health report.
    #[serde(default)]
    pub audio_rebuffer_events: Option<u32>,
    /// Maximum consecutive missing audio frames observed since the previous report.
    #[serde(default)]
    pub audio_max_consecutive_missing_frames: Option<u32>,
    /// Minimum queued audio depth observed since the previous health report.
    #[serde(default)]
    pub audio_queue_min_frames: Option<u32>,
    /// Maximum queued audio depth observed since the previous health report.
    #[serde(default)]
    pub audio_queue_max_frames: Option<u32>,
    /// Latest clock-sync uncertainty estimate in milliseconds.
    #[serde(default)]
    pub clock_uncertainty_ms: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::ClientNetworkQualityReport;
    use serde_json::json;

    #[test]
    fn sustained_audio_health_uses_optional_camel_case_fields() {
        let report = ClientNetworkQualityReport {
            audio_underruns: Some(3),
            audio_rebuffer_events: Some(1),
            audio_max_consecutive_missing_frames: Some(24),
            audio_queue_min_frames: Some(0),
            audio_queue_max_frames: Some(8),
            ..ClientNetworkQualityReport::default()
        };
        let value = serde_json::to_value(report).expect("serialize health report");

        assert_eq!(value["audioUnderruns"], json!(3));
        assert_eq!(value["audioRebufferEvents"], json!(1));
        assert_eq!(value["audioMaxConsecutiveMissingFrames"], json!(24));
        assert_eq!(value["audioQueueMinFrames"], json!(0));
        assert_eq!(value["audioQueueMaxFrames"], json!(8));
    }
}
