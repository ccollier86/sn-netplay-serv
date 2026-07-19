//! Short-lived desktop-to-runner room handoff lifecycle.
//!
//! This module owns arming, preserving, and expiring the one-time window used
//! after an authenticated initial control join. It never parses bearer tokens
//! or performs transport authentication. Handoff deadlines remain private and
//! use the registry's monotonic clock.

use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerRole, PlayerRuntimeState, PlayerStatus, RoomError, RoomStatus,
};
use std::time::{Duration, Instant};

impl NetplayRoom {
    /// Arms a delivered initial join for transfer to a runner process.
    ///
    /// The caller arms before sending `RoomJoined` so a runner claim cannot race
    /// ahead of this marker, then cancels the handoff if delivery fails.
    pub(crate) fn arm_runner_handoff(
        &mut self,
        connection_id: ConnectionId,
        now: Instant,
        grace: Duration,
    ) -> Result<(), RoomError> {
        if matches!(
            self.status,
            RoomStatus::Closed
                | RoomStatus::StartScheduled
                | RoomStatus::Playing
                | RoomStatus::Paused
                | RoomStatus::Recovering
        ) {
            return Err(RoomError::RoomNotReady);
        }

        let slot = self
            .players
            .iter_mut()
            .find(|slot| slot.connection_id == Some(connection_id))
            .ok_or(RoomError::UnknownConnection)?;

        slot.runner_handoff_deadline = Some(now + grace);
        slot.reconnect_room_epoch = Some(self.room_epoch);
        Ok(())
    }

    /// Cancels a handoff whose capability could not be delivered.
    pub(crate) fn cancel_runner_handoff(
        &mut self,
        connection_id: ConnectionId,
    ) -> Result<(), RoomError> {
        let slot = self
            .players
            .iter_mut()
            .find(|slot| slot.connection_id == Some(connection_id))
            .ok_or(RoomError::UnknownConnection)?;

        slot.runner_handoff_deadline = None;
        slot.reconnect_room_epoch = None;
        Ok(())
    }

    /// Preserves an armed slot when its desktop control socket closes.
    ///
    /// Returns `true` only while the handoff deadline is still active. Expired
    /// markers are cleared so the caller can apply ordinary disconnect rules.
    pub(super) fn preserve_runner_handoff_disconnect(
        &mut self,
        connection_id: ConnectionId,
        now: Instant,
    ) -> Result<bool, RoomError> {
        let slot = self
            .players
            .iter_mut()
            .find(|slot| slot.connection_id == Some(connection_id))
            .ok_or(RoomError::UnknownConnection)?;
        let Some(deadline) = slot.runner_handoff_deadline else {
            return Ok(false);
        };

        if now >= deadline {
            slot.runner_handoff_deadline = None;
            return Ok(false);
        }

        let player_index = slot.player_index;
        slot.connection_id = None;
        slot.input_connection_id = None;
        slot.input_socket_token_hash = None;
        slot.input_socket_control_connection_id = None;
        slot.last_seen_at = Some(now);
        slot.latest_local_frame = None;
        slot.latest_local_frame_reported_at = None;
        slot.latest_network_report = None;
        slot.latest_network_reported_at = None;
        slot.status = PlayerStatus::Reconnecting;
        slot.runtime_state = PlayerRuntimeState::Reconnecting;
        slot.reconnect_deadline = None;
        slot.reconnect_room_epoch = Some(self.room_epoch);

        self.compatibility.remove(&player_index);
        self.ready_players.remove(&player_index);
        self.last_input_frames.remove(&player_index);
        self.next_input_frames.remove(&player_index);
        self.reset_sync_state();

        Ok(true)
    }

    /// Expires disconnected handoff slots at their absolute deadline.
    ///
    /// Returns `Some(true)` when a host handoff closed the room, `Some(false)`
    /// when one or more guest slots were cleared, and `None` when no disconnected
    /// handoff changed room membership. Connected expired markers are discarded
    /// without disrupting their still-active control sockets.
    pub(super) fn expire_runner_handoffs(&mut self, now: Instant) -> Option<bool> {
        let expired = self
            .players
            .iter()
            .filter(|slot| {
                slot.runner_handoff_deadline
                    .is_some_and(|deadline| now >= deadline)
            })
            .map(|slot| (slot.player_index, slot.role, slot.connection_id.is_none()))
            .collect::<Vec<_>>();
        let mut guest_cleared = false;

        for (player_index, role, disconnected) in expired {
            if !disconnected {
                if let Some(slot) = self
                    .players
                    .iter_mut()
                    .find(|slot| slot.player_index == player_index)
                {
                    slot.runner_handoff_deadline = None;
                }
                continue;
            }

            let is_host = role == PlayerRole::Host;
            self.clear_disconnected_slot(player_index, is_host);
            if is_host {
                return Some(true);
            }
            guest_cleared = true;
        }

        if guest_cleared {
            self.status = RoomStatus::WaitingForGuest;
            Some(false)
        } else {
            None
        }
    }
}
