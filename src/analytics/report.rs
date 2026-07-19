//! Session-first netplay analytics reports.

use crate::analytics::query::{EventRow, SampleRow, SessionKey};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionReport {
    pub room_id: String,
    pub invite_code: String,
    pub session_epoch: u64,
    pub protocol_version: Option<u16>,
    pub duration_ms: u64,
    pub event_count: u64,
    pub sample_count: u64,
    pub state_hash_matches: u64,
    pub frame_skew_events: u64,
    pub mismatch_diagnostics: u64,
    pub resyncs: u64,
    pub pauses: u64,
    pub reconnect_events: u64,
    pub player_exits: u64,
    pub avg_rtt_ms: Option<f64>,
    pub avg_jitter_ms: Option<f64>,
    pub max_abs_frame_delta: Option<u64>,
    pub total_stalls: u64,
    pub total_catch_up_frames: u64,
    pub total_late_input_frames: u64,
    pub total_audio_underruns: u64,
    pub total_input_resend_frames: u64,
    pub total_input_nacks: u64,
    pub total_replayed_frames: u64,
    pub total_suppressed_audio_frames: u64,
    pub total_suppressed_video_frames: u64,
    pub max_audio_queue_depth_frames: Option<u32>,
    pub total_audio_catch_up_events: u64,
    pub total_audio_trimmed_frames: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FleetReport {
    pub session_count: usize,
    pub protocol_v4_sessions: usize,
    pub protocol_v5_sessions: usize,
    pub unknown_protocol_sessions: usize,
    pub avg_session_duration_ms: f64,
    pub sessions_with_resync: usize,
    pub sessions_with_reconnect: usize,
    pub avg_session_rtt_ms: Option<f64>,
    pub p95_session_rtt_ms: Option<f64>,
    pub avg_session_jitter_ms: Option<f64>,
    pub max_frame_delta_seen: Option<u64>,
    pub total_resyncs: u64,
    pub total_state_hash_matches: u64,
    pub total_mismatch_diagnostics: u64,
    pub total_frame_skew_events: u64,
    pub total_stalls: u64,
    pub total_catch_up_frames: u64,
    pub total_late_input_frames: u64,
    pub total_audio_underruns: u64,
    pub total_input_resend_frames: u64,
    pub total_input_nacks: u64,
    pub total_replayed_frames: u64,
    pub total_suppressed_audio_frames: u64,
    pub total_suppressed_video_frames: u64,
    pub max_audio_queue_depth_frames: Option<u32>,
    pub total_audio_catch_up_events: u64,
    pub total_audio_trimmed_frames: u64,
}

pub fn build_session_reports(
    sessions: &[SessionKey],
    events: &[EventRow],
    samples: &[SampleRow],
) -> Vec<SessionReport> {
    let mut reports = sessions
        .iter()
        .map(|session| {
            let key = session_key(&session.room_id, session.session_epoch);
            (
                key,
                SessionReport {
                    room_id: session.room_id.clone(),
                    invite_code: session.invite_code.clone(),
                    session_epoch: session.session_epoch,
                    duration_ms: session.ended_ms.saturating_sub(session.started_ms),
                    ..SessionReport::default()
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    for event in events {
        let Some(report) = reports.get_mut(&session_key(&event.room_id, event.session_epoch))
        else {
            continue;
        };

        report.event_count += 1;
        report.protocol_version = report.protocol_version.or(event.protocol_version);
        match event.kind.as_str() {
            "stateHashMatched" => report.state_hash_matches += 1,
            "stateHashFrameSkewDiagnostic" => report.frame_skew_events += 1,
            "stateHashMismatchDiagnostic" => report.mismatch_diagnostics += 1,
            "stateHashResyncRequired" | "stateRecoveryCommitted" => report.resyncs += 1,
            "playerExited" => report.player_exits += 1,
            "recoveryStarted" | "playerReconnected" | "recoveryResyncRequired" => {
                report.reconnect_events += 1;
            }
            kind if kind.starts_with("pause") || kind.starts_with("resume") => {
                report.pauses += 1;
            }
            _ => {}
        }
    }

    let mut accumulators = BTreeMap::<String, SessionAccumulator>::new();
    for sample in samples {
        let key = session_key(&sample.room_id, sample.session_epoch);
        let Some(report) = reports.get_mut(&key) else {
            continue;
        };
        let accumulator = accumulators.entry(key).or_default();

        report.sample_count += 1;
        report.protocol_version = report.protocol_version.or(sample.protocol_version);
        accumulator.rtt.add_optional(sample.round_trip_ms);
        accumulator.jitter.add_optional(sample.jitter_ms);
        report.total_stalls += u64::from(sample.stall_count.unwrap_or_default());
        report.total_catch_up_frames += u64::from(sample.catch_up_frames.unwrap_or_default());
        report.total_late_input_frames += u64::from(sample.late_input_frames.unwrap_or_default());
        report.total_audio_underruns += u64::from(sample.audio_underruns.unwrap_or_default());
        report.total_input_resend_frames +=
            u64::from(sample.input_resend_frames.unwrap_or_default());
        report.total_input_nacks += u64::from(sample.input_nacks.unwrap_or_default());
        report.total_replayed_frames += u64::from(sample.replayed_frames.unwrap_or_default());
        report.total_suppressed_audio_frames +=
            u64::from(sample.suppressed_audio_frames.unwrap_or_default());
        report.total_suppressed_video_frames +=
            u64::from(sample.suppressed_video_frames.unwrap_or_default());
        if let Some(depth) = sample.audio_queue_depth_frames {
            report.max_audio_queue_depth_frames =
                Some(report.max_audio_queue_depth_frames.unwrap_or(0).max(depth));
        }
        report.total_audio_catch_up_events +=
            u64::from(sample.audio_catch_up_events.unwrap_or_default());
        report.total_audio_trimmed_frames +=
            u64::from(sample.audio_trimmed_frames.unwrap_or_default());

        if let Some(frame_delta) = sample.frame_delta {
            let abs = frame_delta.unsigned_abs();
            report.max_abs_frame_delta = Some(report.max_abs_frame_delta.unwrap_or(0).max(abs));
        }
    }

    for (key, accumulator) in accumulators {
        if let Some(report) = reports.get_mut(&key) {
            report.avg_rtt_ms = accumulator.rtt.average();
            report.avg_jitter_ms = accumulator.jitter.average();
        }
    }

    sessions
        .iter()
        .filter_map(|session| reports.remove(&session_key(&session.room_id, session.session_epoch)))
        .collect()
}

pub fn build_fleet_report(sessions: &[SessionReport]) -> FleetReport {
    if sessions.is_empty() {
        return FleetReport::default();
    }

    let avg_rtts = sessions
        .iter()
        .filter_map(|session| session.avg_rtt_ms)
        .collect::<Vec<_>>();
    let avg_jitters = sessions
        .iter()
        .filter_map(|session| session.avg_jitter_ms)
        .collect::<Vec<_>>();

    FleetReport {
        session_count: sessions.len(),
        protocol_v4_sessions: sessions
            .iter()
            .filter(|session| session.protocol_version == Some(4))
            .count(),
        protocol_v5_sessions: sessions
            .iter()
            .filter(|session| session.protocol_version == Some(5))
            .count(),
        unknown_protocol_sessions: sessions
            .iter()
            .filter(|session| session.protocol_version.is_none())
            .count(),
        avg_session_duration_ms: average(
            sessions
                .iter()
                .map(|session| session.duration_ms as f64)
                .collect(),
        )
        .unwrap_or_default(),
        sessions_with_resync: sessions
            .iter()
            .filter(|session| session.resyncs > 0)
            .count(),
        sessions_with_reconnect: sessions
            .iter()
            .filter(|session| session.reconnect_events > 0)
            .count(),
        avg_session_rtt_ms: average(avg_rtts.clone()),
        p95_session_rtt_ms: percentile(avg_rtts, 0.95),
        avg_session_jitter_ms: average(avg_jitters),
        max_frame_delta_seen: sessions
            .iter()
            .filter_map(|session| session.max_abs_frame_delta)
            .max(),
        total_resyncs: sessions.iter().map(|session| session.resyncs).sum(),
        total_state_hash_matches: sessions
            .iter()
            .map(|session| session.state_hash_matches)
            .sum(),
        total_mismatch_diagnostics: sessions
            .iter()
            .map(|session| session.mismatch_diagnostics)
            .sum(),
        total_frame_skew_events: sessions
            .iter()
            .map(|session| session.frame_skew_events)
            .sum(),
        total_stalls: sessions.iter().map(|session| session.total_stalls).sum(),
        total_catch_up_frames: sessions
            .iter()
            .map(|session| session.total_catch_up_frames)
            .sum(),
        total_late_input_frames: sessions
            .iter()
            .map(|session| session.total_late_input_frames)
            .sum(),
        total_audio_underruns: sessions
            .iter()
            .map(|session| session.total_audio_underruns)
            .sum(),
        total_input_resend_frames: sessions
            .iter()
            .map(|session| session.total_input_resend_frames)
            .sum(),
        total_input_nacks: sessions
            .iter()
            .map(|session| session.total_input_nacks)
            .sum(),
        total_replayed_frames: sessions
            .iter()
            .map(|session| session.total_replayed_frames)
            .sum(),
        total_suppressed_audio_frames: sessions
            .iter()
            .map(|session| session.total_suppressed_audio_frames)
            .sum(),
        total_suppressed_video_frames: sessions
            .iter()
            .map(|session| session.total_suppressed_video_frames)
            .sum(),
        max_audio_queue_depth_frames: sessions
            .iter()
            .filter_map(|session| session.max_audio_queue_depth_frames)
            .max(),
        total_audio_catch_up_events: sessions
            .iter()
            .map(|session| session.total_audio_catch_up_events)
            .sum(),
        total_audio_trimmed_frames: sessions
            .iter()
            .map(|session| session.total_audio_trimmed_frames)
            .sum(),
    }
}

pub fn print_report(fleet: &FleetReport, sessions: &[SessionReport]) {
    println!("Netplay analytics report");
    println!("sessions: {}", fleet.session_count);
    println!(
        "protocols: {} v4 | {} v5 | {} unknown",
        fleet.protocol_v4_sessions, fleet.protocol_v5_sessions, fleet.unknown_protocol_sessions
    );
    println!(
        "avg session duration: {:.1}s",
        fleet.avg_session_duration_ms / 1000.0
    );
    println!(
        "resyncs: {} total, {} sessions",
        fleet.total_resyncs, fleet.sessions_with_resync
    );
    println!(
        "state hashes: {} matched | {} mismatched | {} frame-skew diagnostics",
        fleet.total_state_hash_matches,
        fleet.total_mismatch_diagnostics,
        fleet.total_frame_skew_events
    );
    println!("reconnect sessions: {}", fleet.sessions_with_reconnect);
    println!(
        "avg RTT/session: {} | p95 RTT/session: {} | avg jitter/session: {}",
        fmt_optional_ms(fleet.avg_session_rtt_ms),
        fmt_optional_ms(fleet.p95_session_rtt_ms),
        fmt_optional_ms(fleet.avg_session_jitter_ms)
    );
    println!(
        "max frame delta: {} | stalls: {} | catch-up frames: {} | late inputs: {} | audio underruns: {}",
        fleet
            .max_frame_delta_seen
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_string()),
        fleet.total_stalls,
        fleet.total_catch_up_frames,
        fleet.total_late_input_frames,
        fleet.total_audio_underruns
    );
    println!(
        "v5 input: {} resent frames | {} NACKs | {} replayed frames",
        fleet.total_input_resend_frames, fleet.total_input_nacks, fleet.total_replayed_frames
    );
    println!(
        "replay/audio: {} audio suppressed | {} video suppressed | max queue {} frames | {} catch-ups | {} trimmed",
        fleet.total_suppressed_audio_frames,
        fleet.total_suppressed_video_frames,
        fleet
            .max_audio_queue_depth_frames
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_string()),
        fleet.total_audio_catch_up_events,
        fleet.total_audio_trimmed_frames
    );
    println!();
    println!(
        "{:<36} {:<6} {:<5} {:<8} {:>7} {:>7} {:>7} {:>7} {:>7} {:>8} {:>8} {:>8}",
        "room_id",
        "epoch",
        "proto",
        "invite",
        "rtt",
        "jitter",
        "match",
        "miss",
        "resync",
        "delta",
        "stalls",
        "audio"
    );

    for session in sessions {
        println!(
            "{:<36} {:<6} {:<5} {:<8} {:>7} {:>7} {:>7} {:>7} {:>7} {:>8} {:>8} {:>8}",
            session.room_id,
            session.session_epoch,
            session
                .protocol_version
                .map(|version| format!("v{version}"))
                .unwrap_or_else(|| "?".to_string()),
            session.invite_code,
            fmt_optional_ms(session.avg_rtt_ms),
            fmt_optional_ms(session.avg_jitter_ms),
            session.state_hash_matches,
            session.mismatch_diagnostics,
            session.resyncs,
            session
                .max_abs_frame_delta
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            session.total_stalls,
            session.total_audio_underruns
        );
    }
}

fn session_key(room_id: &str, session_epoch: u64) -> String {
    format!("{room_id}:{session_epoch}")
}

fn fmt_optional_ms(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}ms"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn average(values: Vec<f64>) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn percentile(mut values: Vec<f64>, percentile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    values.sort_by(|left, right| left.total_cmp(right));
    let index = ((values.len() - 1) as f64 * percentile).round() as usize;

    values.get(index).copied()
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SessionAccumulator {
    rtt: NumericAccumulator,
    jitter: NumericAccumulator,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct NumericAccumulator {
    total: u64,
    count: u64,
}

impl NumericAccumulator {
    fn add_optional(&mut self, value: Option<u32>) {
        if let Some(value) = value {
            self.total += u64::from(value);
            self.count += 1;
        }
    }

    fn average(&self) -> Option<f64> {
        (self.count > 0).then(|| self.total as f64 / self.count as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::query::SessionKey;

    #[test]
    fn session_reports_keep_recent_session_order() {
        let sessions = vec![
            session("newest-room", 2, 2_000),
            session("older-room", 1, 1_000),
        ];

        let reports = build_session_reports(&sessions, &[], &[]);

        assert_eq!(
            reports
                .iter()
                .map(|report| report.room_id.as_str())
                .collect::<Vec<_>>(),
            vec!["newest-room", "older-room"]
        );
    }

    fn session(room_id: &str, session_epoch: u64, ended_ms: u64) -> SessionKey {
        SessionKey {
            room_id: room_id.to_string(),
            session_epoch,
            invite_code: "AB23-CD".to_string(),
            started_ms: 0,
            ended_ms,
        }
    }
}
