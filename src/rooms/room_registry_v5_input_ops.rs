//! Protocol-v5 strict input and host-driven frame relay operations.

use super::InMemoryRoomRegistry;
use crate::protocol::{
    HostFrameOpen, InputCursorNack, InputCursorNackReason, InputCursorResponse, StrictInputBatch,
};
use crate::rooms::{
    ConnectionId, HostFrameOpenOutcome, HostFrameRelayOutcome, InviteCode, RoomError, RoomStatus,
    ScheduledHostFrameReleaseOutcome, StrictInputRelayOutcome,
};

impl InMemoryRoomRegistry {
    pub(super) async fn relay_strict_input_batch_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: StrictInputBatch,
    ) -> Result<StrictInputRelayOutcome, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let batch_context = (
            batch.room_epoch,
            batch.session_epoch,
            batch.player_index,
            batch.start_frame,
            batch.end_frame(),
        );
        let outcome = match stored_room
            .room
            .accept_strict_input_batch(connection_id, batch)
        {
            Ok(outcome) => outcome,
            Err(error) => {
                if matches!(
                    error,
                    RoomError::NotPlaying
                        | RoomError::StaleRoomEpoch
                        | RoomError::StaleSessionEpoch
                ) {
                    let detail = format!(
                        "ignored v5 transition input: expected room/session {}/{}, received {}/{}, player {}, frames {}..={}, next release {}, status {:?}",
                        stored_room.room.room_epoch,
                        stored_room.room.session_epoch,
                        batch_context.0,
                        batch_context.1,
                        batch_context.2.zero_based(),
                        batch_context.3,
                        batch_context.4,
                        stored_room.room.next_release_frame,
                        stored_room.room.status(),
                    );
                    stored_room.record_debug_event(
                        self.clock.now(),
                        "v5TransitionInputIgnored",
                        &detail,
                    );
                    self.record_recent_events(stored_room.debug_events(1));
                    let expected_frame = stored_room
                        .room
                        .next_input_frames
                        .get(&batch_context.2)
                        .copied()
                        .unwrap_or(stored_room.room.sync_start_frame);
                    return Ok(StrictInputRelayOutcome {
                        response: InputCursorResponse::Nack(InputCursorNack {
                            room_epoch: stored_room.room.room_epoch,
                            session_epoch: stored_room.room.session_epoch,
                            player_index: batch_context.2,
                            expected_frame,
                            received_frame: batch_context.3,
                            reason: InputCursorNackReason::SessionState,
                        }),
                        send_response: false,
                        accepted_frame_count: 0,
                        duplicate_frame_count: 0,
                    });
                }
                return Err(error);
            }
        };
        if let InputCursorResponse::Nack(nack) = outcome.response {
            let transition_in_progress = stored_room.room.status() != RoomStatus::Playing
                && (stored_room.room.state_recovery.is_some()
                    || stored_room.room.pause_state.is_some()
                    || matches!(
                        stored_room.room.status(),
                        RoomStatus::StartScheduled
                            | RoomStatus::RepairingState
                            | RoomStatus::Recovering
                    ));
            if transition_in_progress {
                let detail = format!(
                    "ignored v5 transition input response: expected room/session {}/{}, received {}/{}, player {}, expected frame {}, received frames {}..={}, reason {:?}, next release {}, status {:?}",
                    nack.room_epoch,
                    nack.session_epoch,
                    batch_context.0,
                    batch_context.1,
                    nack.player_index.zero_based(),
                    nack.expected_frame,
                    batch_context.3,
                    batch_context.4,
                    nack.reason,
                    stored_room.room.next_release_frame,
                    stored_room.room.status(),
                );
                stored_room.record_debug_event(
                    self.clock.now(),
                    "v5TransitionInputIgnored",
                    &detail,
                );
                self.record_recent_events(stored_room.debug_events(1));
                return Ok(StrictInputRelayOutcome {
                    response: outcome.response,
                    send_response: false,
                    accepted_frame_count: 0,
                    duplicate_frame_count: outcome.duplicate_frame_count,
                });
            }
            let detail = format!(
                "nacked v5 input cursor: room/session {}/{}, player {}, expected frame {}, received frame {}, reason {:?}, batch {}..={}, next release {}, status {:?}",
                nack.room_epoch,
                nack.session_epoch,
                nack.player_index.zero_based(),
                nack.expected_frame,
                nack.received_frame,
                nack.reason,
                batch_context.3,
                batch_context.4,
                stored_room.room.next_release_frame,
                stored_room.room.status(),
            );
            stored_room.record_debug_event(self.clock.now(), "v5InputCursorNacked", &detail);
            self.record_recent_events(stored_room.debug_events(1));
        }
        let accepted_frame_count = outcome
            .accepted_batch
            .as_ref()
            .map_or(0, |batch| batch.payloads.len());
        if let Some(batch) = outcome.accepted_batch {
            stored_room.emit_strict_input_batch(connection_id, batch);
        }
        Ok(StrictInputRelayOutcome {
            response: outcome.response,
            send_response: true,
            accepted_frame_count,
            duplicate_frame_count: outcome.duplicate_frame_count,
        })
    }

    pub(super) async fn relay_host_frame_open_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        open: HostFrameOpen,
    ) -> Result<HostFrameRelayOutcome, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let server_time_ms = self.server_time_ms_at(self.clock.now());
        let open_context = open;
        let outcome = match stored_room.room.accept_host_frame_open(
            connection_id,
            open,
            server_time_ms,
        ) {
            Ok(outcome) => outcome,
            Err(error) => {
                if matches!(
                    error,
                    RoomError::NotPlaying
                        | RoomError::StaleRoomEpoch
                        | RoomError::StaleSessionEpoch
                ) {
                    let detail = format!(
                        "ignored v5 transition host open: expected room/session {}/{}, received {}/{}, expected frame {}, received frame {}, status {:?}",
                        stored_room.room.room_epoch,
                        stored_room.room.session_epoch,
                        open_context.room_epoch,
                        open_context.session_epoch,
                        stored_room.room.next_release_frame,
                        open_context.frame,
                        stored_room.room.status(),
                    );
                    stored_room.record_debug_event(
                        self.clock.now(),
                        "v5TransitionHostOpenIgnored",
                        &detail,
                    );
                    self.record_recent_events(stored_room.debug_events(1));
                    return Ok(HostFrameRelayOutcome::IgnoredTransitionBoundary);
                }
                return Err(error);
            }
        };
        match outcome {
            HostFrameOpenOutcome::Released(release) => {
                stored_room.emit_v5_server_frame(release);
                Ok(HostFrameRelayOutcome::Broadcast)
            }
            HostFrameOpenOutcome::Duplicate(release) => {
                Ok(HostFrameRelayOutcome::Duplicate(release))
            }
            HostFrameOpenOutcome::IgnoredTransitionBoundary => {
                let detail = format!(
                    "ignored v5 transition host open: room/session {}/{}, expected frame {}, received frame {}, status {:?}",
                    stored_room.room.room_epoch,
                    stored_room.room.session_epoch,
                    stored_room.room.next_release_frame,
                    open_context.frame,
                    stored_room.room.status(),
                );
                stored_room.record_debug_event(
                    self.clock.now(),
                    "v5TransitionHostOpenIgnored",
                    &detail,
                );
                self.record_recent_events(stored_room.debug_events(1));
                Ok(HostFrameRelayOutcome::IgnoredTransitionBoundary)
            }
            HostFrameOpenOutcome::Pending { delay_ms } => Ok(HostFrameRelayOutcome::Pending {
                delay_ms,
                room_epoch: stored_room.room.room_epoch,
                session_epoch: stored_room.room.session_epoch,
                frame: stored_room
                    .room
                    .pending_host_frame_open
                    .expect("pending outcome owns one frame"),
            }),
        }
    }

    pub(super) async fn release_scheduled_v5_host_frame_impl(
        &self,
        invite_code: InviteCode,
        room_epoch: u64,
        session_epoch: u64,
        frame: u64,
    ) -> Result<ScheduledHostFrameReleaseOutcome, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        if stored_room.room.room_epoch != room_epoch
            || stored_room.room.session_epoch != session_epoch
            || stored_room.room.pending_host_frame_open != Some(frame)
        {
            return Ok(ScheduledHostFrameReleaseOutcome::Superseded);
        }

        let server_time_ms = self.server_time_ms_at(self.clock.now());
        let scheduled_time_ms = stored_room
            .room
            .scheduled_start()
            .map(|start| start.server_time_ms)
            .ok_or(RoomError::RoomNotReady)?;
        if server_time_ms < scheduled_time_ms {
            return Ok(ScheduledHostFrameReleaseOutcome::RetryAfter(
                scheduled_time_ms - server_time_ms,
            ));
        }
        if !stored_room.emit_due_v5_server_frame(server_time_ms) {
            return Err(RoomError::RoomNotReady);
        }
        Ok(ScheduledHostFrameReleaseOutcome::Released)
    }
}
