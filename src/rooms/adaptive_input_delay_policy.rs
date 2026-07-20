//! Adaptive controller-netplay input-delay policy.
//!
//! The relay uses client health samples to choose delay. The policy is pure so
//! room mutation stays in the room operation module.

use crate::protocol::{
    ClientNetworkQualityReport, DEFAULT_CONTROLLER_INPUT_DELAY_FRAMES, InputDelayChangeReason,
    MAX_CONTROLLER_INPUT_DELAY_FRAMES,
};
use crate::rooms::PlayerSlot;
use std::time::{Duration, Instant};

const MIN_AUTOMATIC_INPUT_DELAY_FRAMES: u8 = 2;
const LATENCY_SAFETY_FRAMES: u8 = 1;
const HIGH_PREDICTION_PRESSURE_FRAMES: u64 = 45;
const HEALTH_SAMPLE_FRESHNESS: Duration = Duration::from_secs(10);
const DEFAULT_NOMINAL_FRAME_RATE_NUMERATOR: u64 = 60;
const DEFAULT_NOMINAL_FRAME_RATE_DENOMINATOR: u64 = 1;

/// Relay decision for a new input-delay value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AdaptiveInputDelayDecision {
    /// New input-delay frame count.
    pub input_delay_frames: u8,
    /// Why this value was selected.
    pub reason: InputDelayChangeReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdaptiveInputDelayTarget {
    all_connected_players_have_fresh_network_samples: bool,
    input_delay_frames: u8,
}

/// Computes startup input-delay decisions from room health.
#[derive(Clone, Debug)]
pub(crate) struct AdaptiveInputDelayPolicy;

impl AdaptiveInputDelayPolicy {
    /// Creates a policy anchored to the room creation time.
    pub fn new(_now: Instant) -> Self {
        Self
    }

    /// Records that a delay change was applied or scheduled.
    pub fn mark_changed(&mut self, _now: Instant) {}

    /// Selects the initial delay when gameplay starts.
    pub fn initial_decision(
        &self,
        current_delay: u8,
        players: &[PlayerSlot],
        room_frame: u64,
        released_frame: Option<u64>,
        now: Instant,
    ) -> Option<AdaptiveInputDelayDecision> {
        let target = target_delay(players, room_frame, released_frame, now)?;

        if target.input_delay_frames == current_delay {
            return None;
        }

        if target.input_delay_frames < current_delay
            && !target.all_connected_players_have_fresh_network_samples
        {
            return None;
        }

        Some(AdaptiveInputDelayDecision {
            input_delay_frames: target.input_delay_frames,
            reason: InputDelayChangeReason::InitialLatency,
        })
    }

    /// Selects the protocol-v5 startup delay from both complete network paths.
    ///
    /// V5 deliberately keeps the configured room value unless every connected
    /// player has a fresh, complete RTT-and-jitter report.
    pub fn initial_v5_decision(
        &self,
        current_delay: u8,
        players: &[PlayerSlot],
        nominal_frame_rate: Option<(u64, u64)>,
        now: Instant,
    ) -> Option<AdaptiveInputDelayDecision> {
        let connected_players = players
            .iter()
            .filter(|slot| slot.connection_id.is_some())
            .collect::<Vec<_>>();
        if connected_players.is_empty() {
            return None;
        }

        let reports = connected_players
            .iter()
            .map(|player| {
                player
                    .latest_network_report
                    .as_ref()
                    .filter(|_| is_fresh(player.latest_network_reported_at, now))
                    .filter(|report| report.round_trip_ms.is_some() && report.jitter_ms.is_some())
            })
            .collect::<Option<Vec<_>>>()?;
        let (frame_rate_numerator, frame_rate_denominator) = nominal_frame_rate
            .filter(|(numerator, denominator)| *numerator > 0 && *denominator > 0)
            .unwrap_or((
                DEFAULT_NOMINAL_FRAME_RATE_NUMERATOR,
                DEFAULT_NOMINAL_FRAME_RATE_DENOMINATOR,
            ));
        let path_budget_ms = reports.iter().fold(0_u64, |budget, report| {
            let round_trip_ms = u64::from(report.round_trip_ms.unwrap_or_default());
            let one_way_ms = round_trip_ms.saturating_add(1) / 2;
            budget.saturating_add(one_way_ms).saturating_add(u64::from(
                report.jitter_ms.expect("complete reports include jitter"),
            ))
        });
        let target =
            frames_for_ms_at_rate(path_budget_ms, frame_rate_numerator, frame_rate_denominator)
                .saturating_add(LATENCY_SAFETY_FRAMES)
                .clamp(
                    MIN_AUTOMATIC_INPUT_DELAY_FRAMES,
                    MAX_CONTROLLER_INPUT_DELAY_FRAMES,
                );

        (target != current_delay).then_some(AdaptiveInputDelayDecision {
            input_delay_frames: target,
            reason: InputDelayChangeReason::InitialLatency,
        })
    }
}

fn target_delay(
    players: &[PlayerSlot],
    room_frame: u64,
    released_frame: Option<u64>,
    now: Instant,
) -> Option<AdaptiveInputDelayTarget> {
    let connected_players = players
        .iter()
        .filter(|slot| slot.connection_id.is_some())
        .collect::<Vec<_>>();
    if connected_players.is_empty() {
        return None;
    }

    let baseline_frame = released_frame.unwrap_or(room_frame);
    let mut saw_sample = false;
    let mut fresh_network_sample_count = 0usize;
    let mut target = MIN_AUTOMATIC_INPUT_DELAY_FRAMES;

    for player in &connected_players {
        if let Some(report_delay) = player
            .latest_network_report
            .as_ref()
            .filter(|_| is_fresh(player.latest_network_reported_at, now))
            .and_then(delay_for_report)
        {
            saw_sample = true;
            fresh_network_sample_count += 1;
            target = target.max(report_delay);
        }

        if let Some(local_frame) = player
            .latest_local_frame
            .filter(|_| is_fresh(player.latest_local_frame_reported_at, now))
        {
            let prediction_frames = local_frame.saturating_sub(baseline_frame);

            if prediction_frames >= HIGH_PREDICTION_PRESSURE_FRAMES {
                saw_sample = true;
                target = target.max(DEFAULT_CONTROLLER_INPUT_DELAY_FRAMES.saturating_add(1));
            }
        }
    }

    saw_sample.then(|| AdaptiveInputDelayTarget {
        all_connected_players_have_fresh_network_samples: fresh_network_sample_count
            == connected_players.len(),
        input_delay_frames: target.clamp(
            MIN_AUTOMATIC_INPUT_DELAY_FRAMES,
            MAX_CONTROLLER_INPUT_DELAY_FRAMES,
        ),
    })
}

fn delay_for_report(report: &ClientNetworkQualityReport) -> Option<u8> {
    let round_trip_ms = report.round_trip_ms?;
    let one_way_ms = round_trip_ms.saturating_add(1) / 2;
    let jitter_ms = report.jitter_ms.unwrap_or(0);
    let runtime_pressure = u8::from(
        report.late_input_frames.unwrap_or(0) > 0 || report.prediction_frames.unwrap_or(0) >= 30,
    );
    let latency_frames = frames_for_ms(one_way_ms.saturating_add(jitter_ms));
    let delay = latency_frames
        .saturating_add(LATENCY_SAFETY_FRAMES)
        .saturating_add(runtime_pressure);

    Some(delay.clamp(
        MIN_AUTOMATIC_INPUT_DELAY_FRAMES,
        MAX_CONTROLLER_INPUT_DELAY_FRAMES,
    ))
}

fn frames_for_ms(milliseconds: u32) -> u8 {
    frames_for_ms_at_rate(
        u64::from(milliseconds),
        DEFAULT_NOMINAL_FRAME_RATE_NUMERATOR,
        DEFAULT_NOMINAL_FRAME_RATE_DENOMINATOR,
    )
}

fn frames_for_ms_at_rate(milliseconds: u64, numerator: u64, denominator: u64) -> u8 {
    let dividend = u128::from(milliseconds).saturating_mul(u128::from(numerator));
    let divisor = u128::from(1_000_u64).saturating_mul(u128::from(denominator));
    let frames = dividend.div_ceil(divisor.max(1));

    frames.min(u128::from(MAX_CONTROLLER_INPUT_DELAY_FRAMES)) as u8
}

fn is_fresh(reported_at: Option<Instant>, now: Instant) -> bool {
    reported_at.is_some_and(|reported_at| {
        now.saturating_duration_since(reported_at) <= HEALTH_SAMPLE_FRESHNESS
    })
}

#[cfg(test)]
mod tests {
    use super::{delay_for_report, frames_for_ms, frames_for_ms_at_rate};
    use crate::protocol::ClientNetworkQualityReport;

    #[test]
    fn converts_latency_to_conservative_frame_delay() {
        let report = ClientNetworkQualityReport {
            round_trip_ms: Some(80),
            jitter_ms: Some(10),
            ..ClientNetworkQualityReport::default()
        };

        assert_eq!(delay_for_report(&report), Some(4));
    }

    #[test]
    fn local_runtime_stalls_do_not_raise_network_delay() {
        let report = ClientNetworkQualityReport {
            round_trip_ms: Some(2),
            jitter_ms: Some(0),
            stall_count: Some(20),
            ..ClientNetworkQualityReport::default()
        };

        assert_eq!(delay_for_report(&report), Some(2));
    }

    #[test]
    fn late_input_raises_network_delay() {
        let report = ClientNetworkQualityReport {
            round_trip_ms: Some(2),
            jitter_ms: Some(0),
            late_input_frames: Some(1),
            ..ClientNetworkQualityReport::default()
        };

        assert_eq!(delay_for_report(&report), Some(3));
    }

    #[test]
    fn rounds_milliseconds_up_to_frames() {
        assert_eq!(frames_for_ms(1), 1);
        assert_eq!(frames_for_ms(17), 2);
        assert_eq!(frames_for_ms_at_rate(17, 50, 1), 1);
        assert_eq!(frames_for_ms_at_rate(20, 60_000, 1_001), 2);
    }
}
