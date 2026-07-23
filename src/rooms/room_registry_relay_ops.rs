//! Runtime relay helpers for the in-memory room registry.
//!
//! This module handles gameplay input, link packets, heartbeats, and
//! coordinated pause/resume messages after room compatibility is established.

use super::InMemoryRoomRegistry;
use crate::protocol::{
    ClientNetworkQualityReport, ClientRuntimeState, FastInputBatch, InputFrame, InputFrameBatch,
    InputFrameLimits, LinkCablePacket, SessionPauseReason, SnapshotLimits, StateHashReport,
    StateRecoveryPin,
};
use crate::rooms::{
    ConnectionId, InputFrameAcceptance, InputFrameCursor, InviteCode, RoomError, RoomView,
    SessionPauseReachedOutcome, SessionResumeOutcome, StateHashEvaluation,
    map_link_cable_data_plane_error,
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
            stored_room.relay_accepted_input_frame(connection_id, input);
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

        let cursors = batch
            .frames
            .iter()
            .map(InputFrameCursor::from_input)
            .collect::<Vec<_>>();
        let acceptances = stored_room.room.accept_input_frame_cursors(
            connection_id,
            &cursors,
            InputFrameLimits::default(),
        )?;

        for (input, acceptance) in batch.frames.into_iter().zip(acceptances) {
            if acceptance == InputFrameAcceptance::Relay {
                stored_room.relay_accepted_input_frame(connection_id, input);
            }
        }

        Ok(())
    }

    /// Validates and broadcasts zero-copy fast input records.
    pub(super) async fn relay_fast_input_batch_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: FastInputBatch,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        if !stored_room
            .room
            .connected_players_support_fast_input_relay()
        {
            return Err(RoomError::RoomNotReady);
        }

        for input in &batch.frames {
            if input.room_epoch != stored_room.room.room_epoch {
                return Err(RoomError::StaleRoomEpoch);
            }

            if input.session_epoch != stored_room.room.session_epoch {
                return Err(RoomError::StaleSessionEpoch);
            }
        }

        let cursors = batch
            .frames
            .iter()
            .map(|frame| InputFrameCursor {
                player_index: frame.player_index,
                frame: frame.frame,
            })
            .collect::<Vec<_>>();
        let acceptances = stored_room.room.accept_input_frame_cursors(
            connection_id,
            &cursors,
            InputFrameLimits::default(),
        )?;

        for (input, acceptance) in batch.frames.into_iter().zip(acceptances) {
            if acceptance == InputFrameAcceptance::Relay {
                stored_room.relay_accepted_fast_input_frame(connection_id, input);
            }
        }

        Ok(())
    }

    /// Validates and targets one SBLK event without holding the registry lock.
    pub(super) async fn relay_link_cable_packet_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        room_epoch: u64,
        session_epoch: u64,
        packet: LinkCablePacket,
    ) -> Result<(), RoomError> {
        let (data_plane, player_index) = {
            let rooms = self.invite_codes.read().await;
            let stored_room = rooms
                .get(invite_code.normalized())
                .ok_or(RoomError::NotFound)?;
            if stored_room.room.room_epoch != room_epoch {
                return Err(RoomError::StaleRoomEpoch);
            }
            if stored_room.room.session_epoch != session_epoch {
                return Err(RoomError::StaleSessionEpoch);
            }
            stored_room
                .room
                .link_cable_data_plane_handle_for_connection(connection_id)?
        };

        let sender_slot = packet.player_index.zero_based();
        let sender_sequence = packet.sequence;
        let result = data_plane.relay(
            connection_id,
            player_index,
            room_epoch,
            session_epoch,
            packet,
        );

        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                let diagnostic_class = error.diagnostic_class();
                let detail = format!(
                    "relay rejected class={diagnostic_class} slot={sender_slot} \
                     sequence={sender_sequence} observedRoomEpoch={room_epoch} \
                     observedSessionEpoch={session_epoch}"
                );
                let mut rooms = self.invite_codes.write().await;
                if let Some(stored_room) = rooms.get_mut(invite_code.normalized()) {
                    stored_room.record_diagnostic_observation(
                        self.clock.now(),
                        "linkCableRelayRejected",
                        &detail,
                    );
                    self.record_recent_events(stored_room.debug_events(1));
                }

                Err(map_link_cable_data_plane_error(error))
            }
        }
    }

    /// Records a deterministic state hash and triggers resync on true drift.
    pub(super) async fn record_state_hash_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        report: StateHashReport,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let evaluation = stored_room
            .room
            .accept_state_hash(connection_id, report, now)?;

        match evaluation {
            StateHashEvaluation::Disabled => {}
            StateHashEvaluation::Pending => {}
            StateHashEvaluation::Matched(frame) => {
                stored_room.record_state_hash_match(now, frame);
                self.record_recent_events(stored_room.debug_events(1));
            }
            StateHashEvaluation::FrameSkew(mismatch) => {
                stored_room.record_state_hash_frame_skew_diagnostic(now, &mismatch);
                self.record_recent_events(stored_room.debug_events(1));
            }
            StateHashEvaluation::TrueMismatch(mismatch) => {
                stored_room.record_state_hash_mismatch_diagnostic(now, &mismatch);
                self.record_recent_events(stored_room.debug_events(1));
            }
            StateHashEvaluation::ResyncRequired(mismatch) => {
                stored_room.emit_state_hash_mismatch(now, mismatch);
                self.record_recent_events(stored_room.debug_events(1));
            }
            StateHashEvaluation::RecoveryPrepare(recovery) => {
                stored_room.emit_state_recovery_prepare(now, recovery);
                self.record_recent_events(stored_room.debug_events(1));
            }
            StateHashEvaluation::RecoveryAttemptLimitExceeded(recovery) => {
                stored_room.emit_state_recovery_failed(
                    now,
                    recovery,
                    "recoveryAttemptLimitExceeded",
                );
                self.record_recent_events(stored_room.debug_events(1));
            }
        }

        Ok(())
    }

    /// Commits a protocol v5 recovery after the host pins exact state.
    pub(super) async fn pin_state_recovery_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        room_epoch: u64,
        session_epoch: u64,
        pin: StateRecoveryPin,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let recovery = stored_room.room.accept_v5_state_recovery_pin_for_epoch(
            connection_id,
            room_epoch,
            session_epoch,
            pin,
            SnapshotLimits::default(),
        )?;
        stored_room.emit_state_recovery_committed(now, recovery);
        self.record_recent_events(stored_room.debug_events(1));
        Ok(())
    }

    /// Records a heartbeat and returns the current room view.
    pub(super) async fn record_heartbeat_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        local_frame: Option<u64>,
        network: Option<ClientNetworkQualityReport>,
        runtime_state: ClientRuntimeState,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();

        stored_room.room.record_heartbeat(
            connection_id,
            now,
            local_frame,
            network.clone(),
            runtime_state,
        )?;
        if let Some(sample) =
            stored_room.performance_sample(now, connection_id, local_frame, network, runtime_state)
        {
            self.record_performance_sample(sample);
        }
        if let Some(change) = stored_room.room.maybe_schedule_adaptive_input_delay(now) {
            stored_room.emit_input_delay_changed(now, change);
            self.record_recent_events(stored_room.debug_events(1));
        }

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
        let now = self.clock.now();
        let server_time_ms = self.server_time_ms_at(now);
        let outcome = stored_room
            .room
            .mark_session_pause_reached_with_outcome_at(
                connection_id,
                sequence,
                paused_at_frame,
                server_time_ms,
            )?;

        match outcome {
            SessionPauseReachedOutcome::Pausing(pause)
            | SessionPauseReachedOutcome::Paused(pause) => {
                stored_room.emit_session_pause_updated(now, pause);
            }
            SessionPauseReachedOutcome::Resumed {
                sequence,
                resume_at_frame,
            } => {
                stored_room.emit_session_resume_scheduled(now, sequence, resume_at_frame, None);
            }
            SessionPauseReachedOutcome::ResumedV5 {
                sequence,
                scheduled_start,
            } => {
                stored_room.emit_session_resume_scheduled(
                    now,
                    sequence,
                    scheduled_start.start_frame,
                    Some(scheduled_start),
                );
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
        let now = self.clock.now();
        let server_time_ms = self.server_time_ms_at(now);
        let outcome = stored_room.room.request_session_resume_with_id_at(
            connection_id,
            request_id,
            reason,
            sequence,
            server_time_ms,
        )?;

        match outcome {
            SessionResumeOutcome::StillPaused(pause) => {
                stored_room.emit_session_pause_updated(now, pause);
            }
            SessionResumeOutcome::Resumed { resume_at_frame } => {
                stored_room.emit_session_resume_scheduled(now, sequence, resume_at_frame, None);
            }
            SessionResumeOutcome::ResumedV5 { scheduled_start } => {
                stored_room.emit_session_resume_scheduled(
                    now,
                    sequence,
                    scheduled_start.start_frame,
                    Some(scheduled_start),
                );
            }
        }
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(room)
    }
}
