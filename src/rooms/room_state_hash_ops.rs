//! Deterministic state-hash desync detection and recovery.
//!
//! Clients periodically report state hashes for the same canonical frame. The
//! relay compares reports only after every connected player reported that frame.

use crate::protocol::{
    AUTHORITATIVE_STATE_HASH_INTERVAL_FRAMES, NearbyStateHashMatchView, PlayerStateHashView,
    StateDigestMode, StateHashMismatchView, StateHashReport, StateRecoveryView,
};
use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerRuntimeState, PlayerStatus, RoomError, RoomStatus,
    StateRecoveryStartOutcome,
};
use std::time::{Duration, Instant};

const STATE_HASH_RETAIN_FRAMES: u64 = 600;
const STATE_HASH_MIN_NEARBY_MATCH_WINDOW: u64 = 8;
const STATE_HASH_NEARBY_MATCH_SLACK_FRAMES: u64 = 4;
const STATE_HASH_MAX_NEARBY_MATCH_WINDOW: u64 = 120;
const STATE_HASH_FRAME_SAMPLE_FRESHNESS: Duration = Duration::from_secs(10);
const STATE_HASH_TRUE_MISMATCHES_BEFORE_RESYNC: u8 = 1;

/// Result of accepting a deterministic state-hash report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StateHashEvaluation {
    /// The negotiated profile does not produce or compare state digests.
    Disabled,
    /// Waiting for every connected player to report the compared frame.
    Pending,
    /// All connected players reported matching state for this frame.
    Matched(u64),
    /// Same-frame hashes differed, but the hash buffer found a nearby match.
    FrameSkew(StateHashMismatchView),
    /// Same-frame hashes differed with no nearby match, below the resync threshold.
    TrueMismatch(StateHashMismatchView),
    /// Confirmed mismatches require clients to resync from host state.
    ResyncRequired(StateHashMismatchView),
    /// Protocol v5 froze the old epoch and requires an exact host snapshot pin.
    RecoveryPrepare(StateRecoveryView),
    /// Protocol v5 closed after repeated authoritative repair attempts.
    RecoveryAttemptLimitExceeded(StateRecoveryView),
}

impl NetplayRoom {
    /// Stores one deterministic state hash and evaluates same/nearby frames.
    pub(super) fn accept_state_hash(
        &mut self,
        connection_id: ConnectionId,
        report: StateHashReport,
        now: Instant,
    ) -> Result<StateHashEvaluation, RoomError> {
        if !self.is_controller_netplay() {
            return Err(RoomError::NotPlaying);
        }

        if !matches!(
            self.status,
            RoomStatus::StartScheduled | RoomStatus::Playing | RoomStatus::Paused
        ) {
            return Err(RoomError::NotPlaying);
        }

        let digest_mode = self.state_digest_mode();
        if digest_mode == StateDigestMode::Disabled {
            return Ok(StateHashEvaluation::Disabled);
        }

        report.validate().map_err(|_| RoomError::InvalidPayload)?;
        let authoritative_v5 =
            self.uses_strict_controller_input() && digest_mode == StateDigestMode::Authoritative;
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        // The host frame-open is what promotes a scheduled epoch to Playing.
        // Ignore a boundary digest that races that promotion instead of
        // tearing down an otherwise valid recovered session.
        if self.status == RoomStatus::StartScheduled {
            return Ok(StateHashEvaluation::Pending);
        }
        if authoritative_v5 {
            if report.frame == 0
                || !report
                    .frame
                    .is_multiple_of(AUTHORITATIVE_STATE_HASH_INTERVAL_FRAMES)
            {
                return Err(RoomError::InvalidPayload);
            }
            if report.frame < self.next_authoritative_state_hash_frame {
                return Ok(StateHashEvaluation::Pending);
            }
            if report.frame != self.next_authoritative_state_hash_frame {
                return Err(RoomError::InvalidPayload);
            }
        }

        let normalized_hash = report.sha256.to_ascii_lowercase();
        let connected_players = self.connected_player_indices();
        let frame_hashes = self.state_hashes.entry(report.frame).or_default();

        frame_hashes.insert(player_index, normalized_hash);

        if !connected_players
            .iter()
            .all(|player_index| frame_hashes.contains_key(player_index))
        {
            return Ok(StateHashEvaluation::Pending);
        }

        let mut hashes = connected_players
            .into_iter()
            .filter_map(|player_index| {
                frame_hashes
                    .get(&player_index)
                    .map(|sha256| PlayerStateHashView {
                        player_index,
                        sha256: sha256.clone(),
                    })
            })
            .collect::<Vec<_>>();

        hashes.sort_by_key(|hash| hash.player_index.zero_based());
        let nearby_matches = if authoritative_v5 {
            Vec::new()
        } else {
            self.nearby_state_hash_matches(
                report.frame,
                &hashes,
                self.dynamic_nearby_state_hash_window(now),
            )
        };
        self.prune_state_hashes(report.frame);

        let Some(first_hash) = hashes.first().map(|hash| hash.sha256.as_str()) else {
            return Ok(StateHashEvaluation::Pending);
        };

        if hashes.iter().all(|hash| hash.sha256 == first_hash) {
            self.reset_state_hash_mismatch_streak();
            if authoritative_v5 {
                self.next_authoritative_state_hash_frame = report
                    .frame
                    .saturating_add(AUTHORITATIVE_STATE_HASH_INTERVAL_FRAMES);
            }
            return Ok(StateHashEvaluation::Matched(report.frame));
        }

        let mismatch = StateHashMismatchView {
            frame: report.frame,
            repair_frame: report.frame,
            hashes,
            nearby_matches,
        };

        if !mismatch.nearby_matches.is_empty() {
            self.record_state_hash_true_mismatch();

            if digest_mode == StateDigestMode::Authoritative
                && self.state_hash_true_mismatch_streak >= STATE_HASH_TRUE_MISMATCHES_BEFORE_RESYNC
            {
                return self.begin_authoritative_recovery(mismatch, now);
            }

            return Ok(StateHashEvaluation::FrameSkew(mismatch));
        }

        self.record_state_hash_true_mismatch();

        if digest_mode == StateDigestMode::Authoritative
            && self.state_hash_true_mismatch_streak >= STATE_HASH_TRUE_MISMATCHES_BEFORE_RESYNC
        {
            return self.begin_authoritative_recovery(mismatch, now);
        }

        Ok(StateHashEvaluation::TrueMismatch(mismatch))
    }

    fn nearby_state_hash_matches(
        &self,
        frame: u64,
        hashes: &[PlayerStateHashView],
        window: u64,
    ) -> Vec<NearbyStateHashMatchView> {
        let mut matches = Vec::new();
        let window = i64::try_from(window).unwrap_or(i64::MAX);

        for source in hashes {
            for target in hashes {
                if source.player_index == target.player_index {
                    continue;
                }

                for offset in -window..=window {
                    if offset == 0 {
                        continue;
                    }

                    let Some(matched_frame) = offset_frame(frame, offset) else {
                        continue;
                    };

                    let Some(frame_hashes) = self.state_hashes.get(&matched_frame) else {
                        continue;
                    };

                    if frame_hashes
                        .get(&target.player_index)
                        .is_some_and(|sha256| sha256 == &source.sha256)
                    {
                        matches.push(NearbyStateHashMatchView {
                            source_player_index: source.player_index,
                            source_frame: frame,
                            matched_player_index: target.player_index,
                            matched_frame,
                            frame_offset: offset,
                        });
                    }
                }
            }
        }

        matches
    }

    fn dynamic_nearby_state_hash_window(&self, now: Instant) -> u64 {
        let observed_spread = self
            .fresh_local_frame_spread(now)
            .into_iter()
            .chain(self.accepted_input_frame_spread())
            .max()
            .unwrap_or(0);

        observed_spread
            .saturating_add(STATE_HASH_NEARBY_MATCH_SLACK_FRAMES)
            .clamp(
                STATE_HASH_MIN_NEARBY_MATCH_WINDOW,
                STATE_HASH_MAX_NEARBY_MATCH_WINDOW,
            )
    }

    fn fresh_local_frame_spread(&self, now: Instant) -> Option<u64> {
        let connected_slots = self
            .players
            .iter()
            .filter(|slot| slot.connection_id.is_some())
            .collect::<Vec<_>>();
        let frames = connected_slots
            .iter()
            .filter_map(|slot| {
                let reported_at = slot.latest_local_frame_reported_at?;

                if now.saturating_duration_since(reported_at) > STATE_HASH_FRAME_SAMPLE_FRESHNESS {
                    return None;
                }

                slot.latest_local_frame
            })
            .collect::<Vec<_>>();

        if frames.len() == connected_slots.len() {
            frame_spread(&frames)
        } else {
            None
        }
    }

    fn accepted_input_frame_spread(&self) -> Option<u64> {
        let frames = self
            .connected_player_indices()
            .into_iter()
            .filter_map(|player_index| self.last_input_frames.get(&player_index).copied())
            .collect::<Vec<_>>();

        frame_spread(&frames)
    }

    fn prune_state_hashes(&mut self, frame: u64) {
        let retain_from = frame.saturating_sub(STATE_HASH_RETAIN_FRAMES);
        self.state_hashes = self.state_hashes.split_off(&retain_from);
    }

    fn enter_state_hash_resync(&mut self, repair_frame: u64) {
        self.reset_sync_state_to(repair_frame);
        self.bump_session_epoch();
        self.status = RoomStatus::CheckingCompatibility;

        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Connected;
                slot.runtime_state = PlayerRuntimeState::Connected;
            });
    }

    fn begin_authoritative_recovery(
        &mut self,
        mismatch: StateHashMismatchView,
        now: Instant,
    ) -> Result<StateHashEvaluation, RoomError> {
        if self.uses_strict_controller_input() {
            return match self.begin_v5_state_recovery(mismatch, now)? {
                StateRecoveryStartOutcome::Preparing(recovery) => {
                    Ok(StateHashEvaluation::RecoveryPrepare(recovery))
                }
                StateRecoveryStartOutcome::AttemptLimitExceeded(recovery) => {
                    Ok(StateHashEvaluation::RecoveryAttemptLimitExceeded(recovery))
                }
            };
        }

        self.enter_state_hash_resync(mismatch.repair_frame);
        Ok(StateHashEvaluation::ResyncRequired(mismatch))
    }

    fn record_state_hash_true_mismatch(&mut self) {
        self.state_hash_true_mismatch_streak =
            self.state_hash_true_mismatch_streak.saturating_add(1);
    }

    fn reset_state_hash_mismatch_streak(&mut self) {
        self.state_hash_true_mismatch_streak = 0;
    }

    pub(super) fn reset_authoritative_state_hash_cursor(&mut self, start_frame: u64) {
        self.next_authoritative_state_hash_frame = next_authoritative_checkpoint_after(start_frame);
    }

    pub(super) fn reset_authoritative_state_hash_cursor_for_resume(&mut self, resume_frame: u64) {
        self.next_authoritative_state_hash_frame =
            next_authoritative_checkpoint_at_or_after(resume_frame);
    }
}

fn next_authoritative_checkpoint_after(frame: u64) -> u64 {
    frame
        .checked_div(AUTHORITATIVE_STATE_HASH_INTERVAL_FRAMES)
        .unwrap_or_default()
        .saturating_add(1)
        .saturating_mul(AUTHORITATIVE_STATE_HASH_INTERVAL_FRAMES)
}

fn next_authoritative_checkpoint_at_or_after(frame: u64) -> u64 {
    if frame == 0 {
        return AUTHORITATIVE_STATE_HASH_INTERVAL_FRAMES;
    }

    frame
        .div_ceil(AUTHORITATIVE_STATE_HASH_INTERVAL_FRAMES)
        .saturating_mul(AUTHORITATIVE_STATE_HASH_INTERVAL_FRAMES)
}

fn frame_spread(frames: &[u64]) -> Option<u64> {
    if frames.len() < 2 {
        return None;
    }

    let min = frames.iter().min()?;
    let max = frames.iter().max()?;

    Some(max.saturating_sub(*min))
}

fn offset_frame(frame: u64, offset: i64) -> Option<u64> {
    if offset.is_negative() {
        frame.checked_sub(offset.unsigned_abs())
    } else {
        frame.checked_add(offset as u64)
    }
}
