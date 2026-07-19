//! Protocol-v5 strict input and host-driven frame relay operations.

use super::InMemoryRoomRegistry;
use crate::protocol::{HostFrameOpen, InputCursorResponse, StrictInputBatch};
use crate::rooms::{
    ConnectionId, HostFrameOpenOutcome, HostFrameRelayOutcome, InviteCode, RoomError,
};

impl InMemoryRoomRegistry {
    pub(super) async fn relay_strict_input_batch_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: StrictInputBatch,
    ) -> Result<InputCursorResponse, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let outcome = stored_room
            .room
            .accept_strict_input_batch(connection_id, batch)?;
        if let Some(batch) = outcome.accepted_batch {
            stored_room.emit_strict_input_batch(connection_id, batch);
        }
        Ok(outcome.response)
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
        match stored_room
            .room
            .accept_host_frame_open(connection_id, open, server_time_ms)?
        {
            HostFrameOpenOutcome::Released(release) => {
                stored_room.emit_v5_server_frame(release);
                Ok(HostFrameRelayOutcome::Broadcast)
            }
            HostFrameOpenOutcome::Duplicate(release) => {
                Ok(HostFrameRelayOutcome::Duplicate(release))
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
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        if stored_room.room.room_epoch != room_epoch
            || stored_room.room.session_epoch != session_epoch
            || stored_room.room.pending_host_frame_open != Some(frame)
        {
            return Ok(());
        }

        let server_time_ms = self.server_time_ms_at(self.clock.now());
        if !stored_room.emit_due_v5_server_frame(server_time_ms) {
            return Err(RoomError::RoomNotReady);
        }
        Ok(())
    }
}
