//! Runtime relay helpers for the in-memory room registry.
//!
//! This module handles gameplay input, link packets, heartbeats, and
//! coordinated pause/resume messages after room compatibility is established.

use super::InMemoryRoomRegistry;
use crate::protocol::{
    ClientRuntimeState, InputFrame, InputFrameBatch, InputFrameLimits, LinkCablePacket,
    LinkCablePacketLimits, SessionPauseReason,
};
use crate::rooms::{
    ConnectionId, InputFrameAcceptance, InviteCode, RoomError, RoomView,
    SessionPauseReachedOutcome, SessionResumeOutcome,
};

impl InMemoryRoomRegistry {
    /// Validates and broadcasts one controller input frame.
    pub(super) async fn relay_input_frame_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        input: InputFrame,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let acceptance = stored_room.room.accept_input_frame(
            connection_id,
            &input,
            InputFrameLimits::default(),
        )?;

        if acceptance == InputFrameAcceptance::Relay {
            let now = self.clock.now();
            stored_room.emit_input_frame(now, connection_id, input);
        }

        Ok(())
    }

    /// Validates and broadcasts a binary batch of controller input frames.
    pub(super) async fn relay_input_frame_batch_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: InputFrameBatch,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        if stored_room.room.room_epoch != batch.room_epoch {
            return Err(RoomError::StaleRoomEpoch);
        }

        if stored_room.room.session_epoch != batch.session_epoch {
            return Err(RoomError::StaleSessionEpoch);
        }

        let mut next_room = stored_room.room.clone();
        let mut accepted_frames = Vec::with_capacity(batch.frames.len());

        for input in batch.frames {
            let acceptance =
                next_room.accept_input_frame(connection_id, &input, InputFrameLimits::default())?;

            if acceptance == InputFrameAcceptance::Relay {
                accepted_frames.push(input);
            }
        }

        if !accepted_frames.is_empty() {
            stored_room.room = next_room;
            let now = self.clock.now();
            stored_room.emit_input_frame_batch(
                now,
                connection_id,
                InputFrameBatch {
                    frames: accepted_frames,
                    ..batch
                },
            );
        }

        Ok(())
    }

    /// Validates and broadcasts one virtual link-cable packet.
    pub(super) async fn relay_link_cable_packet_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        packet: LinkCablePacket,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room.room.accept_link_cable_packet(
            connection_id,
            &packet,
            LinkCablePacketLimits::default(),
        )?;
        let now = self.clock.now();
        stored_room.emit_link_cable_packet(now, connection_id, packet);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(())
    }

    /// Records a heartbeat and returns the current room view.
    pub(super) async fn record_heartbeat_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        runtime_state: ClientRuntimeState,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();

        stored_room
            .room
            .record_heartbeat(connection_id, now, runtime_state)?;

        Ok(stored_room.view(now))
    }

    /// Schedules or extends a coordinated pause.
    pub(super) async fn request_session_pause_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        request_id: String,
        reason: SessionPauseReason,
        local_frame: u64,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let already_paused = stored_room.view(now).pause.is_some();
        let pause = stored_room.room.request_session_pause_with_id(
            connection_id,
            request_id,
            reason,
            local_frame,
        )?;

        if already_paused {
            stored_room.emit_session_pause_updated(now, pause);
        } else {
            stored_room.emit_session_pause_scheduled(now, pause);
        }
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(room)
    }

    /// Records that one client reached the coordinated pause frame.
    pub(super) async fn mark_session_pause_reached_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        sequence: u64,
        paused_at_frame: u64,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let outcome = stored_room.room.mark_session_pause_reached_with_outcome(
            connection_id,
            sequence,
            paused_at_frame,
        )?;
        let now = self.clock.now();

        match outcome {
            SessionPauseReachedOutcome::Pausing(pause)
            | SessionPauseReachedOutcome::Paused(pause) => {
                stored_room.emit_session_pause_updated(now, pause);
            }
            SessionPauseReachedOutcome::Resumed {
                sequence,
                resume_at_frame,
            } => {
                stored_room.emit_session_resume_scheduled(now, sequence, resume_at_frame);
            }
        }
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(room)
    }

    /// Releases one pause holder and schedules resume when all holders release.
    pub(super) async fn request_session_resume_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        request_id: String,
        reason: SessionPauseReason,
        sequence: u64,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let outcome = stored_room.room.request_session_resume_with_id(
            connection_id,
            request_id,
            reason,
            sequence,
        )?;
        let now = self.clock.now();

        match outcome {
            SessionResumeOutcome::StillPaused(pause) => {
                stored_room.emit_session_pause_updated(now, pause);
            }
            SessionResumeOutcome::Resumed { resume_at_frame } => {
                stored_room.emit_session_resume_scheduled(now, sequence, resume_at_frame);
            }
        }
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(room)
    }
}
