//! Deterministic state-hash desync detection and recovery.
//!
//! Clients periodically report state hashes for the same canonical frame. The
//! relay compares reports only after every connected player reported that frame.

use crate::protocol::{PlayerStateHashView, StateHashMismatchView, StateHashReport};
use crate::rooms::{ConnectionId, NetplayRoom, RoomError, RoomStatus};

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
            }))
        }
    }
    fn prune_state_hashes(&mut self, frame: u64) {
        let retain_from = frame.saturating_sub(120);
        self.state_hashes = self.state_hashes.split_off(&retain_from);
    }
}
