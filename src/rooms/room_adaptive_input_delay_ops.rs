//! Room operations for relay-owned startup input delay.
//!
//! The policy decides a target delay before gameplay starts. Runtime delay
//! changes stay disabled so active rooms follow the relay frame clock without
//! mid-session latency retuning.

use crate::protocol::{ClientNetworkQualityReport, InputDelayChange};
use crate::rooms::NetplayRoom;
use std::time::Instant;

impl NetplayRoom {
    /// Stores a client network sample for later delay decisions.
    pub(super) fn record_network_report(
        &mut self,
        connection_id: crate::rooms::ConnectionId,
        local_frame: Option<u64>,
        network: Option<ClientNetworkQualityReport>,
        now: Instant,
    ) {
        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.connection_id == Some(connection_id))
        {
            if let Some(local_frame) = local_frame {
                slot.latest_local_frame = Some(local_frame);
                slot.latest_local_frame_reported_at = Some(now);
            }
            if let Some(network) = network {
                slot.latest_network_report = Some(network);
                slot.latest_network_reported_at = Some(now);
            }
        }
    }

    /// Applies the relay-selected startup delay before gameplay starts.
    pub(super) fn apply_initial_adaptive_input_delay(&mut self, now: Instant) {
        let Some(decision) = self.input_delay_policy.initial_decision(
            self.session.controller.input_delay_frames,
            &self.players,
            self.room_frame,
            self.released_frame,
            now,
        ) else {
            return;
        };

        self.session.controller.input_delay_frames = decision.input_delay_frames;
        self.input_delay_policy.mark_changed(now);
    }

    /// Applies the complete two-path protocol-v5 delay before start scheduling.
    pub(super) fn apply_initial_v5_input_delay(&mut self, now: Instant) {
        if !self.uses_strict_controller_input() {
            return;
        }

        let nominal_frame_rate = self
            .compatibility
            .values()
            .find_map(|fingerprint| fingerprint.valid_determinism_v5())
            .map(|profile| {
                (
                    profile.nominal_frame_rate_numerator,
                    profile.nominal_frame_rate_denominator,
                )
            });
        let Some(decision) = self.input_delay_policy.initial_v5_decision(
            self.session.controller.input_delay_frames,
            &self.players,
            nominal_frame_rate,
            now,
        ) else {
            return;
        };

        self.session.controller.input_delay_frames = decision.input_delay_frames;
        self.input_delay_policy.mark_changed(now);
    }

    /// Runtime delay changes are intentionally disabled for active rooms.
    pub(super) fn maybe_schedule_adaptive_input_delay(
        &mut self,
        _now: Instant,
    ) -> Option<InputDelayChange> {
        None
    }

    /// Applies the pending delay once the relay clock reaches its frame.
    pub(super) fn apply_pending_input_delay_if_due(&mut self) {
        let Some(change) = self.pending_input_delay_change.as_ref() else {
            return;
        };

        if self.next_release_frame < change.effective_frame {
            return;
        }

        self.session.controller.input_delay_frames = change.input_delay_frames;
        self.pending_input_delay_change = None;
    }
}
