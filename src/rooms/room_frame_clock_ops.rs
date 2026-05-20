//! Relay-owned controller frame clock operations.
//!
//! The room model stores the cursors, while this module owns the rules for
//! releasing exactly one canonical server frame and building frame diagnostics.

use crate::protocol::ServerFrame;
use crate::rooms::{NetplayRoom, PlayerFrameCursorView, RoomFrameClockView, RoomStatus};

impl NetplayRoom {
    /// Releases the next relay-owned server frame if the canonical cursor allows it.
    pub(super) fn release_next_server_frame(&mut self) -> Option<ServerFrame> {
        if self.status != RoomStatus::Playing {
            return None;
        }

        if self.next_release_frame > self.room_frame {
            return None;
        }

        if !self.connected_player_indices().iter().all(|player_index| {
            self.last_input_frames
                .get(player_index)
                .is_some_and(|frame| *frame >= self.next_release_frame)
        }) {
            return None;
        }

        let frame = self.next_release_frame;

        self.next_release_frame = self.next_release_frame.saturating_add(1);
        self.released_frame = Some(frame);

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
        }
    }
}
