//! Coordinated pause state for one active room.
//!
//! This object tracks one pause lifecycle at a time. It is independent from
//! socket transport code so Desktop and Android share the same relay behavior.

use crate::protocol::{
    SessionPauseHolder, SessionPauseReason, SessionPauseState, SessionPauseView,
};
use crate::rooms::PlayerIndex;
use std::collections::{HashMap, HashSet};

/// Mutable pause state for one active coordinated pause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SessionPauseStateTracker {
    acknowledged_players: HashSet<PlayerIndex>,
    holders: HashMap<PlayerIndex, SessionPauseReason>,
    pause_at_frame: u64,
    pause_request_ids: HashSet<String>,
    paused_at_frame: Option<u64>,
    reason: SessionPauseReason,
    resume_request_ids: HashSet<String>,
    requested_by_player_index: PlayerIndex,
    sequence: u64,
}

impl SessionPauseStateTracker {
    /// Creates a new pause lifecycle.
    pub(super) fn new(
        sequence: u64,
        request_id: String,
        reason: SessionPauseReason,
        requested_by_player_index: PlayerIndex,
        pause_at_frame: u64,
    ) -> Self {
        let mut holders = HashMap::new();
        holders.insert(requested_by_player_index, reason);
        let mut pause_request_ids = HashSet::new();
        if !request_id.is_empty() {
            pause_request_ids.insert(request_id);
        }

        Self {
            acknowledged_players: HashSet::new(),
            holders,
            pause_at_frame,
            pause_request_ids,
            paused_at_frame: None,
            reason,
            resume_request_ids: HashSet::new(),
            requested_by_player_index,
            sequence,
        }
    }

    /// Adds or updates a player pause holder.
    pub(super) fn hold(
        &mut self,
        player_index: PlayerIndex,
        request_id: String,
        reason: SessionPauseReason,
    ) {
        if !request_id.is_empty() && !self.pause_request_ids.insert(request_id) {
            return;
        }

        self.holders.insert(player_index, reason);
    }

    /// Marks one player as having reached the scheduled pause frame.
    pub(super) fn acknowledge(&mut self, player_index: PlayerIndex, paused_at_frame: u64) {
        self.acknowledged_players.insert(player_index);
        self.paused_at_frame = Some(
            self.paused_at_frame
                .map_or(paused_at_frame, |current| current.max(paused_at_frame)),
        );
    }

    /// Releases a player's pause holder.
    pub(super) fn release(&mut self, player_index: PlayerIndex, request_id: String) {
        if !request_id.is_empty() && !self.resume_request_ids.insert(request_id) {
            return;
        }

        self.holders.remove(&player_index);
    }

    /// Returns whether the pause sequence matches.
    pub(super) fn has_sequence(&self, sequence: u64) -> bool {
        self.sequence == sequence
    }

    /// Returns the scheduled pause frame.
    pub(super) fn pause_at_frame(&self) -> u64 {
        self.pause_at_frame
    }

    /// Returns the resume frame after all holders are released.
    pub(super) fn resume_at_frame(&self) -> u64 {
        self.paused_at_frame
            .unwrap_or(self.pause_at_frame)
            .saturating_add(1)
    }

    /// Returns whether every connected player acknowledged this pause.
    pub(super) fn every_connected_player_acknowledged(
        &self,
        connected_players: &[PlayerIndex],
    ) -> bool {
        !connected_players.is_empty()
            && connected_players
                .iter()
                .all(|player_index| self.acknowledged_players.contains(player_index))
    }

    /// Returns whether any client still holds the pause.
    pub(super) fn has_holders(&self) -> bool {
        !self.holders.is_empty()
    }

    /// Creates a serializable pause view.
    pub(super) fn view(&self, state: SessionPauseState) -> SessionPauseView {
        let mut acknowledged_player_indexes: Vec<_> =
            self.acknowledged_players.iter().copied().collect();
        acknowledged_player_indexes.sort();

        let mut holders: Vec<_> = self
            .holders
            .iter()
            .map(|(player_index, reason)| SessionPauseHolder {
                player_index: *player_index,
                reason: *reason,
            })
            .collect();
        holders.sort_by_key(|holder| holder.player_index);

        SessionPauseView {
            acknowledged_player_indexes,
            holders,
            pause_at_frame: self.pause_at_frame,
            paused_at_frame: self.paused_at_frame,
            reason: self.reason,
            requested_by_player_index: self.requested_by_player_index,
            sequence: self.sequence,
            state,
        }
    }
}
