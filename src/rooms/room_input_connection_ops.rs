//! Binary input-socket attachment and recovery operations for active rooms.
//!
//! These operations are separate from the control socket lifecycle so the room
//! model can evolve dual-channel relay behavior without growing the generic
//! connection module.

use crate::rooms::{ConnectionId, NetplayRoom, PlayerIndex, RoomError, RoomStatus};
use std::time::{Duration, Instant};

impl NetplayRoom {
    /// Attaches the binary input socket to an occupied player slot.
    pub fn attach_input_socket(
        &mut self,
        player_index: PlayerIndex,
        room_epoch: u64,
        session_epoch: u64,
        input_socket_token_hash: &str,
        input_connection_id: ConnectionId,
        now: Instant,
    ) -> Result<(), RoomError> {
        if !self.is_controller_netplay() {
            return Err(RoomError::NotPlaying);
        }

        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        if self.room_epoch != room_epoch {
            return Err(RoomError::StaleRoomEpoch);
        }

        if self.session_epoch != session_epoch {
            return Err(RoomError::StaleSessionEpoch);
        }

        let slot = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
            .ok_or(RoomError::UnknownConnection)?;

        if slot.input_socket_token_hash.as_deref() != Some(input_socket_token_hash) {
            return Err(RoomError::ResumeTokenInvalid);
        }

        let Some(control_connection_id) = slot.connection_id else {
            return Err(RoomError::ResumeTokenInvalid);
        };
        if slot.input_socket_control_connection_id != Some(control_connection_id)
            || slot.input_connection_id.is_some()
        {
            return Err(RoomError::ResumeTokenInvalid);
        }

        slot.input_socket_token_hash = None;
        slot.input_socket_control_connection_id = None;
        slot.input_connection_id = Some(input_connection_id);
        slot.last_seen_at = Some(now);

        Ok(())
    }

    /// Detaches a binary input socket and starts recovery when gameplay depended on it.
    pub fn disconnect_input_socket(
        &mut self,
        input_connection_id: ConnectionId,
        now: Instant,
        reconnect_grace: Duration,
    ) -> Result<bool, RoomError> {
        if !self.is_controller_netplay() {
            return Err(RoomError::NotPlaying);
        }

        let recoverable = matches!(
            self.status,
            RoomStatus::StartScheduled
                | RoomStatus::Playing
                | RoomStatus::Paused
                | RoomStatus::Recovering
        );
        let (player_index, control_connection_id) = {
            let slot = self
                .players
                .iter_mut()
                .find(|slot| slot.input_connection_id == Some(input_connection_id))
                .ok_or(RoomError::UnknownConnection)?;
            let control_connection_id = slot.connection_id;

            slot.input_connection_id = None;
            slot.input_socket_token_hash = None;
            slot.input_socket_control_connection_id = None;
            slot.last_seen_at = Some(now);

            (slot.player_index, control_connection_id)
        };
        self.last_input_frames.remove(&player_index);
        self.next_input_frames.remove(&player_index);

        if recoverable {
            let control_connection_id =
                control_connection_id.ok_or(RoomError::UnknownConnection)?;
            self.disconnect_with_recovery(control_connection_id, now, reconnect_grace)?;
            return Ok(true);
        }

        Ok(false)
    }
}
