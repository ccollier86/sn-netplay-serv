//! Protocol v5 strict controller-input cursor operations.

use crate::limits::V5_MAX_FUTURE_FRAME_DISTANCE;
use crate::protocol::{
    InputCursorAck, InputCursorNack, InputCursorNackReason, InputCursorResponse, StrictInputBatch,
};
use crate::rooms::{ConnectionId, NetplayRoom, RoomError, RoomStatus};

/// Result of one strict input mutation.
pub(crate) struct StrictInputBatchOutcome {
    pub response: InputCursorResponse,
    pub accepted_batch: Option<StrictInputBatch>,
}

impl NetplayRoom {
    /// Atomically accepts only the exact next input cursor for protocol v5.
    pub(crate) fn accept_strict_input_batch(
        &mut self,
        connection_id: ConnectionId,
        batch: StrictInputBatch,
    ) -> Result<StrictInputBatchOutcome, RoomError> {
        self.validate_strict_input_envelope(connection_id, &batch)?;
        let player_index = batch.player_index;
        let expected = self
            .next_input_frames
            .get(&player_index)
            .copied()
            .unwrap_or(self.sync_start_frame);
        let end_frame = batch
            .start_frame
            .checked_add(batch.payloads.len().saturating_sub(1) as u64)
            .ok_or(RoomError::InvalidPayload)?;

        if !self.strict_input_state_accepts(end_frame) {
            return Ok(self.strict_input_nack(
                player_index,
                expected,
                end_frame,
                InputCursorNackReason::SessionState,
            ));
        }
        if batch.start_frame > expected {
            return Ok(self.strict_input_nack(
                player_index,
                expected,
                batch.start_frame,
                InputCursorNackReason::InputGap,
            ));
        }
        let maximum_frame = self
            .next_release_frame
            .saturating_add(V5_MAX_FUTURE_FRAME_DISTANCE);
        if end_frame > maximum_frame {
            return Ok(self.strict_input_nack(
                player_index,
                expected,
                maximum_frame.saturating_add(1),
                InputCursorNackReason::FutureFrameTooLarge,
            ));
        }

        let first_new_frame = expected.max(batch.start_frame);
        let accepted_batch = if first_new_frame <= end_frame {
            let payload_offset = usize::try_from(first_new_frame - batch.start_frame)
                .map_err(|_| RoomError::InvalidPayload)?;
            let payloads = batch.payloads[payload_offset..].to_vec();
            let next_expected_frame = end_frame.checked_add(1).ok_or(RoomError::InvalidPayload)?;
            self.last_input_frames.insert(player_index, end_frame);
            self.next_input_frames
                .insert(player_index, next_expected_frame);
            Some(StrictInputBatch {
                start_frame: first_new_frame,
                payloads,
                ..batch
            })
        } else {
            None
        };
        let next_expected_frame = self
            .next_input_frames
            .get(&player_index)
            .copied()
            .unwrap_or(expected);

        Ok(StrictInputBatchOutcome {
            response: InputCursorResponse::Ack(InputCursorAck {
                room_epoch: self.room_epoch,
                session_epoch: self.session_epoch,
                player_index,
                next_expected_frame,
            }),
            accepted_batch,
        })
    }

    fn validate_strict_input_envelope(
        &self,
        connection_id: ConnectionId,
        batch: &StrictInputBatch,
    ) -> Result<(), RoomError> {
        if !self.uses_strict_controller_input() {
            return Err(RoomError::InvalidPayload);
        }
        if batch.room_epoch != self.room_epoch {
            return Err(RoomError::StaleRoomEpoch);
        }
        if batch.session_epoch != self.session_epoch {
            return Err(RoomError::StaleSessionEpoch);
        }
        let owned = self
            .player_index_for_input_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        if owned != batch.player_index {
            return Err(RoomError::SlotSpoofing(batch.player_index));
        }
        Ok(())
    }

    fn strict_input_state_accepts(&self, frame: u64) -> bool {
        if !matches!(
            self.status,
            RoomStatus::SyncingState
                | RoomStatus::Ready
                | RoomStatus::StartScheduled
                | RoomStatus::Playing
                | RoomStatus::Paused
        ) {
            return false;
        }
        self.pause_state.as_ref().is_none_or(|pause| {
            frame
                <= pause
                    .pause_at_frame()
                    .saturating_add(u64::from(self.session.controller.input_delay_frames))
        })
    }

    fn strict_input_nack(
        &self,
        player_index: crate::rooms::PlayerIndex,
        expected_frame: u64,
        received_frame: u64,
        reason: InputCursorNackReason,
    ) -> StrictInputBatchOutcome {
        StrictInputBatchOutcome {
            response: InputCursorResponse::Nack(InputCursorNack {
                room_epoch: self.room_epoch,
                session_epoch: self.session_epoch,
                player_index,
                expected_frame,
                received_frame,
                reason,
            }),
            accepted_batch: None,
        }
    }
}
