//! Relay-owned controller frame clock operations.
//!
//! The room model stores the cursors, while this module owns the rules for
//! releasing exactly one canonical server frame and building frame diagnostics.

use crate::protocol::ServerFrame;
use crate::rooms::{NetplayRoom, PlayerFrameCursorView, RoomFrameClockView, RoomStatus};

impl NetplayRoom {
    /// Releases the next relay-owned server frame if the canonical cursor allows it.
    pub(super) fn release_next_server_frame(&mut self, server_time_ms: u64) -> Option<ServerFrame> {
        if self.uses_strict_controller_input() {
            return None;
        }
        if self.status == RoomStatus::StartScheduled {
            let start = self.scheduled_start()?;
            if server_time_ms < start.server_time_ms {
                return None;
            }
            self.status = RoomStatus::Playing;
            self.players
                .iter_mut()
                .filter(|slot| slot.connection_id.is_some())
                .for_each(|slot| {
                    slot.status = crate::rooms::PlayerStatus::Playing;
                    slot.runtime_state = crate::rooms::PlayerRuntimeState::Playing;
                });
        }

        if self.status != RoomStatus::Playing {
            return None;
        }

        self.apply_pending_input_delay_if_due();

        let frame = self.next_release_frame;
        if !self.host_has_input_for_frame(frame) {
            return None;
        }

        self.next_release_frame = self.next_release_frame.saturating_add(1);
        self.released_frame = Some(frame);
        self.room_frame = self.room_frame.max(frame);

        Some(ServerFrame {
            room_epoch: self.room_epoch,
            session_epoch: self.session_epoch,
            frame,
            canonical_frame: self.room_frame,
        })
    }

    /// Creates frame-clock diagnostics for room views and admin snapshots.
    pub(super) fn frame_clock_view(&self) -> RoomFrameClockView {
        RoomFrameClockView {
            canonical_frame: self.room_frame,
            released_frame: self.released_frame,
            next_release_frame: self.next_release_frame,
            accepted_inputs: self
                .players
                .iter()
                .filter(|slot| !slot.is_empty())
                .map(|slot| PlayerFrameCursorView {
                    player_index: slot.player_index.zero_based(),
                    frame: self.last_input_frames.get(&slot.player_index).copied(),
                })
                .collect(),
            pending_input_delay_change: self.pending_input_delay_change.clone(),
        }
    }

    fn host_has_input_for_frame(&self, frame: u64) -> bool {
        self.players
            .iter()
            .find(|slot| slot.role == crate::rooms::PlayerRole::Host)
            .and_then(|slot| self.last_input_frames.get(&slot.player_index))
            .is_some_and(|input_frame| *input_frame >= frame)
    }
}
