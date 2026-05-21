//! Deterministic state-hash desync detection and recovery.
//!
//! Clients periodically report state hashes for the same canonical frame. The
//! relay compares reports only after every connected player reported that frame.

use crate::protocol::{
    NearbyStateHashMatchView, PlayerStateHashView, StateHashMismatchView, StateHashReport,
};
use crate::rooms::{ConnectionId, NetplayRoom, RoomError, RoomStatus};

const STATE_HASH_RETAIN_FRAMES: u64 = 600;
const STATE_HASH_NEARBY_MATCH_WINDOW: i64 = 2;

impl NetplayRoom {
    /// Stores one deterministic state hash and returns a mismatch once all
    /// connected players reported the same frame.
    pub(super) fn accept_state_hash(
        &mut self,
        connection_id: ConnectionId,
        report: StateHashReport,
    ) -> Result<Option<StateHashMismatchView>, RoomError> {
        if self.status != RoomStatus::Playing && self.status != RoomStatus::Paused {
            return Err(RoomError::NotPlaying);
        }

        report.validate().map_err(|_| RoomError::InvalidPayload)?;
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let normalized_hash = report.sha256.to_ascii_lowercase();
        let connected_players = self.connected_player_indices();
        let frame_hashes = self.state_hashes.entry(report.frame).or_default();

        frame_hashes.insert(player_index, normalized_hash);

        if !connected_players
            .iter()
            .all(|player_index| frame_hashes.contains_key(player_index))
        {
            return Ok(None);
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
        let nearby_matches = self.nearby_state_hash_matches(report.frame, &hashes);
        self.prune_state_hashes(report.frame);

        let Some(first_hash) = hashes.first().map(|hash| hash.sha256.as_str()) else {
            return Ok(None);
        };

        if hashes.iter().all(|hash| hash.sha256 == first_hash) {
            Ok(None)
        } else {
            Ok(Some(StateHashMismatchView {
                frame: report.frame,
                hashes,
                nearby_matches,
            }))
        }
    }

    fn nearby_state_hash_matches(
        &self,
        frame: u64,
        hashes: &[PlayerStateHashView],
    ) -> Vec<NearbyStateHashMatchView> {
        let mut matches = Vec::new();

        for source in hashes {
            for target in hashes {
                if source.player_index == target.player_index {
                    continue;
                }

                for offset in -STATE_HASH_NEARBY_MATCH_WINDOW..=STATE_HASH_NEARBY_MATCH_WINDOW {
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

    fn prune_state_hashes(&mut self, frame: u64) {
        let retain_from = frame.saturating_sub(STATE_HASH_RETAIN_FRAMES);
        self.state_hashes = self.state_hashes.split_off(&retain_from);
    }
}

fn offset_frame(frame: u64, offset: i64) -> Option<u64> {
    if offset.is_negative() {
        frame.checked_sub(offset.unsigned_abs())
    } else {
        frame.checked_add(offset as u64)
    }
}
