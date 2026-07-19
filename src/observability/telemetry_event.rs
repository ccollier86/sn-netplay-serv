//! Serializable durable telemetry events.
//!
//! These records are derived from sanitized room debug events. They must not
//! contain tokens, raw input payloads, snapshot bytes, or license secrets.

use crate::lobbies::LobbyDebugEvent;
use crate::rooms::{RoomDebugEvent, RoomId, RoomPerformanceSample};
use serde::Serialize;

/// Queue item written by the async telemetry drain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetplayTelemetryRecord {
    /// Sanitized lifecycle/debug event.
    RoomEvent(NetplayTelemetryEvent),
    /// Sanitized persistent lobby event.
    LobbyEvent(NetplayLobbyTelemetryEvent),
    /// Sanitized heartbeat/runtime sample.
    PerformanceSample(NetplayPerformanceSample),
}

/// Append-only event row written to analytics storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NetplayTelemetryEvent {
    /// Milliseconds since unix epoch.
    pub timestamp_ms: u64,
    /// Stable internal room id.
    pub room_id: RoomId,
    /// Human invite code for operator correlation.
    pub invite_code: String,
    /// Monotonic event sequence inside the room.
    pub event_seq: u64,
    /// Current room epoch.
    pub room_epoch: u64,
    /// Current session epoch.
    pub session_epoch: u64,
    /// Exact room protocol that produced this event.
    pub protocol_version: u16,
    /// Stable event kind.
    pub kind: String,
    /// Sanitized detail string.
    pub detail: String,
}

impl From<RoomDebugEvent> for NetplayTelemetryEvent {
    fn from(event: RoomDebugEvent) -> Self {
        Self {
            timestamp_ms: u64::try_from(event.timestamp_ms).unwrap_or(u64::MAX),
            room_id: event.room_id,
            invite_code: event.invite_code,
            event_seq: event.event_seq,
            room_epoch: event.room_epoch,
            session_epoch: event.session_epoch,
            protocol_version: event.protocol_version,
            kind: event.kind,
            detail: event.detail,
        }
    }
}

/// Append-only lobby event row written to analytics storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NetplayLobbyTelemetryEvent {
    /// Milliseconds since unix epoch.
    pub timestamp_ms: u64,
    /// Stable internal lobby id.
    pub lobby_id: RoomId,
    /// Human invite code for operator correlation.
    pub invite_code: String,
    /// Monotonic event sequence inside the lobby.
    pub event_seq: u64,
    /// Current lobby epoch.
    pub lobby_epoch: u64,
    /// Stable event kind.
    pub kind: String,
    /// Sanitized detail string.
    pub detail: String,
}

impl From<LobbyDebugEvent> for NetplayLobbyTelemetryEvent {
    fn from(event: LobbyDebugEvent) -> Self {
        Self {
            timestamp_ms: u64::try_from(event.timestamp_ms).unwrap_or(u64::MAX),
            lobby_id: event.lobby_id,
            invite_code: event.invite_code,
            event_seq: event.event_seq,
            lobby_epoch: event.lobby_epoch,
            kind: event.kind,
            detail: event.detail,
        }
    }
}

/// Append-only performance sample row written to analytics storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NetplayPerformanceSample {
    pub timestamp_ms: u64,
    pub room_id: RoomId,
    pub invite_code: String,
    pub event_seq: u64,
    pub room_epoch: u64,
    pub session_epoch: u64,
    pub protocol_version: u16,
    pub player_index: u8,
    pub runtime_state: String,
    pub local_frame: Option<u64>,
    pub canonical_frame: u64,
    pub released_frame: Option<u64>,
    pub next_release_frame: u64,
    pub accepted_input_frame: Option<u64>,
    pub frame_delta: Option<i64>,
    pub round_trip_ms: Option<u32>,
    pub jitter_ms: Option<u32>,
    pub prediction_frames: Option<u32>,
    pub stall_count: Option<u32>,
    pub catch_up_frames: Option<u32>,
    pub late_input_frames: Option<u32>,
    pub audio_underruns: Option<u32>,
    pub input_resend_frames: Option<u32>,
    pub input_nacks: Option<u32>,
    pub replayed_frames: Option<u32>,
    pub suppressed_audio_frames: Option<u32>,
    pub suppressed_video_frames: Option<u32>,
    pub audio_queue_depth_frames: Option<u32>,
    pub audio_catch_up_events: Option<u32>,
    pub audio_trimmed_frames: Option<u32>,
}

impl From<RoomPerformanceSample> for NetplayPerformanceSample {
    fn from(sample: RoomPerformanceSample) -> Self {
        let network = sample.network.unwrap_or_default();

        Self {
            timestamp_ms: u64::try_from(sample.timestamp_ms).unwrap_or(u64::MAX),
            room_id: sample.room_id,
            invite_code: sample.invite_code,
            event_seq: sample.event_seq,
            room_epoch: sample.room_epoch,
            session_epoch: sample.session_epoch,
            protocol_version: sample.protocol_version,
            player_index: sample.player_index,
            runtime_state: serde_json::to_value(sample.runtime_state)
                .ok()
                .and_then(|value| value.as_str().map(ToString::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
            local_frame: sample.local_frame,
            canonical_frame: sample.canonical_frame,
            released_frame: sample.released_frame,
            next_release_frame: sample.next_release_frame,
            accepted_input_frame: sample.accepted_input_frame,
            frame_delta: sample.frame_delta,
            round_trip_ms: network.round_trip_ms,
            jitter_ms: network.jitter_ms,
            prediction_frames: network.prediction_frames,
            stall_count: network.stall_count,
            catch_up_frames: network.catch_up_frames,
            late_input_frames: network.late_input_frames,
            audio_underruns: network.audio_underruns,
            input_resend_frames: network.input_resend_frames,
            input_nacks: network.input_nacks,
            replayed_frames: network.replayed_frames,
            suppressed_audio_frames: network.suppressed_audio_frames,
            suppressed_video_frames: network.suppressed_video_frames,
            audio_queue_depth_frames: network.audio_queue_depth_frames,
            audio_catch_up_events: network.audio_catch_up_events,
            audio_trimmed_frames: network.audio_trimmed_frames,
        }
    }
}
