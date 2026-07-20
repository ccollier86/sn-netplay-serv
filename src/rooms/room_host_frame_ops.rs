//! Protocol v5 host-driven frame-open operations.

use crate::protocol::{AcceptedInputCursor, HostFrameOpen, ServerFrameReleaseV5};
use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerIndex, PlayerRole, PlayerRuntimeState, PlayerStatus,
    RoomError, RoomStatus,
};

/// Result of one host frame-open declaration.
pub(crate) enum HostFrameOpenOutcome {
    Released(ServerFrameReleaseV5),
    Duplicate(ServerFrameReleaseV5),
    IgnoredTransitionBoundary,
    Pending { delay_ms: u64 },
}

/// Registry result used by the input transport to own a one-shot start wake.
pub enum HostFrameRelayOutcome {
    /// A new release was already broadcast to every input socket.
    Broadcast,
    /// An old host open receives the latest cumulative release directly.
    Duplicate(ServerFrameReleaseV5),
    /// Ordered work queued before a coordinated pause or recovery is obsolete.
    IgnoredTransitionBoundary,
    /// The first open is held until its scheduled one-shot wake.
    Pending {
        /// Wall-clock delay before the scheduled server deadline.
        delay_ms: u64,
        /// Room ownership epoch pinned by the wake.
        room_epoch: u64,
        /// Deterministic session epoch pinned by the wake.
        session_epoch: u64,
        /// Exact pending first frame pinned by the wake.
        frame: u64,
    },
}

/// Result of one transport-owned scheduled first-frame wake.
pub enum ScheduledHostFrameReleaseOutcome {
    /// The exact pending frame was released and broadcast.
    Released,
    /// The room, epoch, or pending frame no longer owns this wake.
    Superseded,
    /// Clock rounding left a small bounded delay before the exact deadline.
    RetryAfter(u64),
}

impl NetplayRoom {
    /// Applies an exact host frame open or returns its idempotent prior release.
    pub(crate) fn accept_host_frame_open(
        &mut self,
        connection_id: ConnectionId,
        open: HostFrameOpen,
        server_time_ms: u64,
    ) -> Result<HostFrameOpenOutcome, RoomError> {
        self.validate_host_frame_sender(connection_id)?;

        if open.room_epoch != self.room_epoch || open.session_epoch != self.session_epoch {
            return Ok(self
                .current_v5_release()
                .map(HostFrameOpenOutcome::Duplicate)
                .unwrap_or(HostFrameOpenOutcome::IgnoredTransitionBoundary));
        }

        if self.status == RoomStatus::RepairingState {
            return Ok(HostFrameOpenOutcome::IgnoredTransitionBoundary);
        }
        if open.frame < self.next_release_frame {
            return self
                .current_v5_release()
                .map(HostFrameOpenOutcome::Duplicate)
                .ok_or(RoomError::OutOfOrderFrame);
        }
        if self
            .pause_state
            .as_ref()
            .is_some_and(|pause| open.frame > pause.pause_at_frame())
        {
            return Ok(HostFrameOpenOutcome::IgnoredTransitionBoundary);
        }
        if self.status == RoomStatus::StartScheduled && open.frame > self.next_release_frame {
            return Ok(HostFrameOpenOutcome::IgnoredTransitionBoundary);
        }
        if !matches!(
            self.status,
            RoomStatus::StartScheduled | RoomStatus::Playing
        ) {
            return Ok(HostFrameOpenOutcome::IgnoredTransitionBoundary);
        }
        if open.frame > self.next_release_frame || !self.host_input_covers(open.frame) {
            return Err(RoomError::OutOfOrderFrame);
        }

        if self.status == RoomStatus::StartScheduled {
            let start = self.scheduled_start().ok_or(RoomError::RoomNotReady)?;
            if open.frame != start.start_frame {
                return Err(RoomError::OutOfOrderFrame);
            }
            let scheduled_time_ms = start.server_time_ms;
            if server_time_ms < scheduled_time_ms {
                self.pending_host_frame_open = Some(open.frame);
                return Ok(HostFrameOpenOutcome::Pending {
                    delay_ms: scheduled_time_ms - server_time_ms,
                });
            }
            self.enter_v5_playing();
        } else if self.status != RoomStatus::Playing {
            return Ok(HostFrameOpenOutcome::IgnoredTransitionBoundary);
        }

        Ok(HostFrameOpenOutcome::Released(
            self.release_v5_host_frame(open.frame),
        ))
    }

    /// Releases an early first host open once its scheduled server time arrives.
    pub(crate) fn release_due_v5_host_frame(
        &mut self,
        server_time_ms: u64,
    ) -> Option<ServerFrameReleaseV5> {
        let frame = self.pending_host_frame_open?;
        let start = self.scheduled_start()?;
        if self.status != RoomStatus::StartScheduled || server_time_ms < start.server_time_ms {
            return None;
        }
        self.pending_host_frame_open = None;
        self.enter_v5_playing();
        Some(self.release_v5_host_frame(frame))
    }

    fn validate_host_frame_sender(&self, connection_id: ConnectionId) -> Result<(), RoomError> {
        if !self.uses_strict_controller_input() {
            return Err(RoomError::InvalidPayload);
        }
        let owned = self
            .player_index_for_input_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let is_host = self
            .players
            .iter()
            .any(|slot| slot.player_index == owned && slot.role == PlayerRole::Host);
        is_host.then_some(()).ok_or(RoomError::HostOnly)
    }

    fn host_input_covers(&self, frame: u64) -> bool {
        self.next_input_frames
            .get(&PlayerIndex::ONE)
            .copied()
            .unwrap_or(self.sync_start_frame)
            > frame
    }

    fn enter_v5_playing(&mut self) {
        self.status = RoomStatus::Playing;
        self.finish_v5_state_recovery();
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Playing;
                slot.runtime_state = PlayerRuntimeState::Playing;
            });
    }

    fn release_v5_host_frame(&mut self, frame: u64) -> ServerFrameReleaseV5 {
        self.pending_host_frame_open = None;
        self.next_release_frame = frame.saturating_add(1);
        self.released_frame = Some(frame);
        self.room_frame = self.room_frame.max(frame);
        self.current_v5_release()
            .expect("released frame exists after host open")
    }

    fn current_v5_release(&self) -> Option<ServerFrameReleaseV5> {
        Some(ServerFrameReleaseV5 {
            room_epoch: self.room_epoch,
            session_epoch: self.session_epoch,
            released_frame: self.released_frame?,
            next_host_frame: self.next_release_frame,
            accepted_inputs: self
                .players
                .iter()
                .filter(|slot| !slot.is_empty())
                .map(|slot| AcceptedInputCursor {
                    player_index: slot.player_index,
                    next_expected_frame: self
                        .next_input_frames
                        .get(&slot.player_index)
                        .copied()
                        .unwrap_or(self.sync_start_frame),
                })
                .collect(),
        })
    }
}
