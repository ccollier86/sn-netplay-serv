//! Controller-netplay input and coordinated pause operations.
//!
//! This module keeps frame input and app-menu pause handling out of the general
//! room lifecycle model.

use crate::protocol::{
    InputFrame, InputFrameLimits, NetplaySessionMode, SessionPauseReason, SessionPauseState,
    SessionPauseView,
};
use crate::rooms::{
    ConnectionId, InputFrameAcceptance, NetplayRoom, PlayerIndex, PlayerRuntimeState, PlayerStatus,
    RoomError, RoomStatus, SessionPauseStateTracker,
};

const SESSION_PAUSE_LEAD_FRAMES: u64 = 8;

impl NetplayRoom {
    /// Validates and records an input frame from one connection.
    pub fn accept_input_frame(
        &mut self,
        connection_id: ConnectionId,
        input: &InputFrame,
        limits: InputFrameLimits,
    ) -> Result<InputFrameAcceptance, RoomError> {
        if self.session.mode != NetplaySessionMode::ControllerNetplay {
            return Err(RoomError::NotPlaying);
        }

        let owned_index = self
            .player_index_for_input_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        if owned_index != input.player_index {
            return Err(RoomError::SlotSpoofing(input.player_index));
        }

        let acceptance = self.input_frame_acceptance(input.frame)?;

        if acceptance == InputFrameAcceptance::Ignore {
            return Ok(InputFrameAcceptance::Ignore);
        }

        if input.frame > self.room_frame + limits.max_future_frame_distance {
            return Err(RoomError::FutureFrameTooLarge);
        }

        let next_frame = self.next_input_frame_for_player(input.player_index);

        if input.frame < next_frame {
            return Ok(InputFrameAcceptance::Ignore);
        }

        // Bounded gaps can happen at startup or after resync if a client begins
        // relaying from its current runtime frame. Peers fill those missing
        // frames with predicted input, so the relay must not tear down the room.
        self.last_input_frames
            .insert(input.player_index, input.frame);
        self.next_input_frames
            .insert(input.player_index, input.frame.saturating_add(1));

        Ok(InputFrameAcceptance::Relay)
    }

    /// Schedules or extends a coordinated session pause.
    pub fn request_session_pause(
        &mut self,
        connection_id: ConnectionId,
        reason: SessionPauseReason,
        local_frame: u64,
    ) -> Result<SessionPauseView, RoomError> {
        self.request_session_pause_with_id(connection_id, String::new(), reason, local_frame)
    }

    /// Schedules or extends a coordinated pause with an idempotency key.
    pub fn request_session_pause_with_id(
        &mut self,
        connection_id: ConnectionId,
        request_id: String,
        reason: SessionPauseReason,
        local_frame: u64,
    ) -> Result<SessionPauseView, RoomError> {
        if self.status != RoomStatus::Playing && self.status != RoomStatus::Paused {
            return Err(RoomError::NotPlaying);
        }

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let current_pause_state = self.current_pause_state();

        if let Some(pause_state) = self.pause_state.as_mut() {
            pause_state.hold(player_index, request_id, reason);
            return Ok(pause_state.view(current_pause_state));
        }

        let sequence = self.next_pause_sequence;
        self.next_pause_sequence = self.next_pause_sequence.saturating_add(1);
        let pause_at_frame = self
            .room_frame
            .max(local_frame)
            .saturating_add(SESSION_PAUSE_LEAD_FRAMES);

        let pause_state = SessionPauseStateTracker::new(
            sequence,
            request_id,
            reason,
            player_index,
            pause_at_frame,
        );
        let view = pause_state.view(SessionPauseState::Pausing);
        self.pause_state = Some(pause_state);

        Ok(view)
    }

    /// Records that a player reached the scheduled pause frame.
    pub fn mark_session_pause_reached(
        &mut self,
        connection_id: ConnectionId,
        sequence: u64,
        paused_at_frame: u64,
    ) -> Result<SessionPauseView, RoomError> {
        match self.mark_session_pause_reached_with_outcome(
            connection_id,
            sequence,
            paused_at_frame,
        )? {
            SessionPauseReachedOutcome::Pausing(pause)
            | SessionPauseReachedOutcome::Paused(pause) => Ok(pause),
            SessionPauseReachedOutcome::Resumed { .. } => Err(RoomError::RoomNotReady),
        }
    }

    /// Records a pause acknowledgement and returns whether pending resume can run.
    pub fn mark_session_pause_reached_with_outcome(
        &mut self,
        connection_id: ConnectionId,
        sequence: u64,
        paused_at_frame: u64,
    ) -> Result<SessionPauseReachedOutcome, RoomError> {
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let connected_players = self.connected_player_indices();
        let pause_state = self.pause_state.as_mut().ok_or(RoomError::RoomNotReady)?;

        if !pause_state.has_sequence(sequence) || paused_at_frame < pause_state.pause_at_frame() {
            return Err(RoomError::RoomNotReady);
        }

        pause_state.acknowledge(player_index, paused_at_frame);

        if pause_state.every_connected_player_acknowledged(&connected_players) {
            if !pause_state.has_holders() {
                let resume_at_frame = pause_state.resume_at_frame();
                self.pause_state = None;
                self.status = RoomStatus::Playing;
                self.players
                    .iter_mut()
                    .filter(|slot| slot.connection_id.is_some())
                    .for_each(|slot| {
                        slot.status = PlayerStatus::Playing;
                        slot.runtime_state = PlayerRuntimeState::Playing;
                    });
                return Ok(SessionPauseReachedOutcome::Resumed {
                    resume_at_frame,
                    sequence,
                });
            }

            self.status = RoomStatus::Paused;
            self.players
                .iter_mut()
                .filter(|slot| slot.connection_id.is_some())
                .for_each(|slot| {
                    slot.status = PlayerStatus::Paused;
                    slot.runtime_state = PlayerRuntimeState::Paused;
                });
            return self
                .pause_state
                .as_ref()
                .map(|pause_state| {
                    SessionPauseReachedOutcome::Paused(pause_state.view(SessionPauseState::Paused))
                })
                .ok_or(RoomError::RoomNotReady);
        }

        self.pause_state
            .as_ref()
            .map(|pause_state| {
                SessionPauseReachedOutcome::Pausing(pause_state.view(SessionPauseState::Pausing))
            })
            .ok_or(RoomError::RoomNotReady)
    }

    /// Releases one pause holder and returns resume details when gameplay can resume.
    pub fn request_session_resume(
        &mut self,
        connection_id: ConnectionId,
        sequence: u64,
    ) -> Result<SessionResumeOutcome, RoomError> {
        self.request_session_resume_with_id(
            connection_id,
            String::new(),
            SessionPauseReason::Menu,
            sequence,
        )
    }

    /// Releases one pause holder using an idempotency key.
    pub fn request_session_resume_with_id(
        &mut self,
        connection_id: ConnectionId,
        request_id: String,
        _reason: SessionPauseReason,
        sequence: u64,
    ) -> Result<SessionResumeOutcome, RoomError> {
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let current_pause_state = self.current_pause_state();
        let pause_state = self.pause_state.as_mut().ok_or(RoomError::RoomNotReady)?;

        if !pause_state.has_sequence(sequence) {
            return Err(RoomError::RoomNotReady);
        }

        pause_state.release(player_index, request_id);

        if pause_state.has_holders() {
            return Ok(SessionResumeOutcome::StillPaused(
                pause_state.view(current_pause_state),
            ));
        }

        if self.status != RoomStatus::Paused {
            return Ok(SessionResumeOutcome::StillPaused(
                pause_state.view(current_pause_state),
            ));
        }

        let resume_at_frame = pause_state.resume_at_frame();
        self.pause_state = None;
        self.status = RoomStatus::Playing;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Playing;
                slot.runtime_state = PlayerRuntimeState::Playing;
            });

        Ok(SessionResumeOutcome::Resumed { resume_at_frame })
    }

    pub(super) fn current_pause_state(&self) -> SessionPauseState {
        if self.status == RoomStatus::Paused {
            SessionPauseState::Paused
        } else {
            SessionPauseState::Pausing
        }
    }

    fn input_frame_acceptance(&self, frame: u64) -> Result<InputFrameAcceptance, RoomError> {
        match self.status {
            RoomStatus::Playing => Ok(self.pause_frame_acceptance(frame)),
            RoomStatus::Paused => Ok(self.pause_frame_acceptance(frame)),
            _ => Err(RoomError::NotPlaying),
        }
    }

    fn pause_frame_acceptance(&self, frame: u64) -> InputFrameAcceptance {
        let Some(pause_state) = self.pause_state.as_ref() else {
            return InputFrameAcceptance::Relay;
        };
        let accept_through = pause_state
            .pause_at_frame()
            .saturating_add(u64::from(self.session.controller.input_delay_frames));

        if frame <= accept_through {
            InputFrameAcceptance::Relay
        } else {
            InputFrameAcceptance::Ignore
        }
    }

    fn next_input_frame_for_player(&self, player_index: PlayerIndex) -> u64 {
        self.next_input_frames
            .get(&player_index)
            .copied()
            .unwrap_or(self.sync_start_frame)
    }
}

/// Result of releasing one pause holder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionResumeOutcome {
    /// Other holders still keep the room paused.
    StillPaused(SessionPauseView),
    /// All holders released and the room can resume.
    Resumed {
        /// Frame clients resume from.
        resume_at_frame: u64,
    },
}

/// Result of a client acknowledging a scheduled pause frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionPauseReachedOutcome {
    /// Pause is still waiting for other players.
    Pausing(SessionPauseView),
    /// Every connected player reached the pause and at least one holder remains.
    Paused(SessionPauseView),
    /// Every player reached the pause and all holders were already released.
    Resumed {
        /// Pause sequence that completed.
        sequence: u64,
        /// Frame clients should resume from.
        resume_at_frame: u64,
    },
}
