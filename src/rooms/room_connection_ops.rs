//! Connection lifecycle and recovery operations for active rooms.
//!
//! This module owns slot attachment, disconnection, resume-token recovery, and
//! heartbeat-derived liveness state. It intentionally avoids compatibility,
//! snapshot, and input relay rules.

use crate::auth::VerifiedLicense;
use crate::protocol::ClientRuntimeState;
use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerIndex, PlayerRole, PlayerRuntimeState, PlayerStatus,
    ResumeTokenHash, RoomError, RoomStatus,
};
use std::time::{Duration, Instant};

impl NetplayRoom {
    /// Adds a guest to the first empty slot and returns their player index.
    pub fn join_guest(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        self.join_guest_with_resume(license, connection_id, String::new(), Instant::now())
    }

    /// Adds a guest and stores the resume-token hash for future reconnects.
    pub fn join_guest_with_resume(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
        resume_token_hash: ResumeTokenHash,
        now: Instant,
    ) -> Result<PlayerIndex, RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let player_index = {
            let slot = self
                .players
                .iter_mut()
                .find(|candidate| candidate.is_empty())
                .ok_or(RoomError::RoomFull)?;
            slot.occupy_guest(&license, connection_id, resume_token_hash, now);
            slot.player_index
        };
        self.reset_sync_for_checking_compatibility();

        Ok(player_index)
    }

    /// Attaches a socket connection to the reserved host slot.
    pub fn attach_host(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        self.attach_host_with_resume(license, connection_id, String::new(), Instant::now())
    }

    /// Attaches a host socket and stores a fresh resume-token hash.
    pub fn attach_host_with_resume(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
        resume_token_hash: ResumeTokenHash,
        now: Instant,
    ) -> Result<PlayerIndex, RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let slot = self
            .players
            .iter_mut()
            .find(|candidate| candidate.role == PlayerRole::Host)
            .ok_or(RoomError::UnknownConnection)?;
        let subject_matches = slot
            .subject_key
            .as_deref()
            .is_some_and(|subject_key| subject_key == license.identity_key());

        if !subject_matches {
            return Err(RoomError::HostSubjectMismatch);
        }

        slot.connection_id = Some(connection_id);
        slot.status = PlayerStatus::Connected;
        slot.runtime_state = PlayerRuntimeState::Connected;
        slot.resume_token_hash = Some(resume_token_hash);
        slot.reconnect_deadline = None;
        slot.last_seen_at = Some(now);
        self.ready_players.remove(&slot.player_index);

        Ok(slot.player_index)
    }

    /// Marks the connection as disconnected and returns whether the room closed.
    pub fn disconnect(&mut self, connection_id: ConnectionId) -> Result<bool, RoomError> {
        self.disconnect_with_recovery(connection_id, Instant::now(), Duration::from_secs(0))
    }

    /// Marks the connection disconnected and starts recovery when appropriate.
    pub fn disconnect_with_recovery(
        &mut self,
        connection_id: ConnectionId,
        now: Instant,
        reconnect_grace: Duration,
    ) -> Result<bool, RoomError> {
        let slot = self
            .players
            .iter_mut()
            .find(|slot| slot.connection_id == Some(connection_id))
            .ok_or(RoomError::UnknownConnection)?;
        let player_index = slot.player_index;
        let is_host = slot.role == PlayerRole::Host;
        let recoverable = matches!(self.status, RoomStatus::Playing | RoomStatus::Paused);

        slot.connection_id = None;
        slot.last_seen_at = Some(now);
        self.compatibility.remove(&player_index);
        self.ready_players.remove(&player_index);
        self.last_input_frames.remove(&player_index);

        if recoverable {
            self.mark_slot_reconnecting(player_index, now, reconnect_grace);
            return Ok(false);
        }

        self.clear_disconnected_slot(player_index, is_host);
        if is_host {
            return Ok(true);
        }

        self.status = RoomStatus::WaitingForGuest;
        Ok(false)
    }

    /// Reclaims a disconnected player slot with a matching resume token hash.
    pub fn reconnect_player(
        &mut self,
        player_index: PlayerIndex,
        resume_token_hash: &str,
        connection_id: ConnectionId,
        now: Instant,
    ) -> Result<(), RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let slot = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
            .ok_or(RoomError::UnknownConnection)?;

        if slot.resume_token_hash.as_deref() != Some(resume_token_hash) {
            return Err(RoomError::ResumeTokenInvalid);
        }

        if let Some(deadline) = slot.reconnect_deadline
            && now > deadline
        {
            slot.status = PlayerStatus::RecoveryExpired;
            slot.runtime_state = PlayerRuntimeState::RecoveryExpired;
            return Err(RoomError::RecoveryExpired);
        }

        slot.connection_id = Some(connection_id);
        slot.status = PlayerStatus::Connected;
        slot.runtime_state = PlayerRuntimeState::Reconnecting;
        slot.last_seen_at = Some(now);
        slot.reconnect_deadline = None;
        self.reset_sync_for_checking_compatibility();

        Ok(())
    }

    /// Records a heartbeat from a connected player.
    pub fn record_heartbeat(
        &mut self,
        connection_id: ConnectionId,
        now: Instant,
        runtime_state: ClientRuntimeState,
    ) -> Result<(), RoomError> {
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let runtime_state = runtime_state.into();

        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
        {
            slot.last_seen_at = Some(now);
            slot.runtime_state = runtime_state;
        }

        Ok(())
    }

    /// Starts recovery for sockets whose heartbeat exceeded the disconnect window.
    pub fn recover_stale_connections(
        &mut self,
        now: Instant,
        heartbeat_disconnect: Duration,
        reconnect_grace: Duration,
    ) -> bool {
        if !matches!(self.status, RoomStatus::Playing | RoomStatus::Paused) {
            return false;
        }

        let stale_players = self
            .players
            .iter()
            .filter(|slot| slot.connection_id.is_some())
            .filter(|slot| {
                slot.last_seen_at
                    .is_some_and(|last_seen| now.duration_since(last_seen) >= heartbeat_disconnect)
            })
            .map(|slot| slot.player_index)
            .collect::<Vec<_>>();

        if stale_players.is_empty() {
            return false;
        }

        for player_index in stale_players {
            self.compatibility.remove(&player_index);
            self.ready_players.remove(&player_index);
            self.last_input_frames.remove(&player_index);
            if let Some(slot) = self
                .players
                .iter_mut()
                .find(|slot| slot.player_index == player_index)
            {
                slot.connection_id = None;
                slot.last_seen_at = Some(now);
                slot.status = PlayerStatus::Reconnecting;
                slot.runtime_state = PlayerRuntimeState::Reconnecting;
                slot.reconnect_deadline = Some(now + reconnect_grace);
            }
        }

        self.enter_recovery_state();
        true
    }

    /// Returns whether a recovering room has exceeded a player recovery deadline.
    pub fn is_recovery_expired(&self, now: Instant) -> bool {
        self.status == RoomStatus::Recovering
            && self.players.iter().any(|slot| {
                slot.reconnect_deadline
                    .is_some_and(|deadline| now >= deadline)
            })
    }

    /// Returns whether all occupied slots have been disconnected long enough.
    pub fn is_idle_disconnected(&self, now: Instant, idle_timeout: Duration) -> bool {
        let occupied = self.players.iter().filter(|slot| !slot.is_empty());
        occupied.clone().any(|_| true)
            && occupied
                .filter_map(|slot| slot.last_seen_at)
                .all(|last_seen| now.duration_since(last_seen) >= idle_timeout)
            && self.players.iter().all(|slot| slot.connection_id.is_none())
    }

    fn reset_sync_for_checking_compatibility(&mut self) {
        self.reset_sync_state();
        self.bump_room_epoch();
        self.bump_session_epoch();
        self.status = RoomStatus::CheckingCompatibility;
        self.players
            .iter_mut()
            .filter(|slot| !slot.is_empty() && slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Connected;
                slot.runtime_state = PlayerRuntimeState::Connected;
            });
    }

    fn mark_slot_reconnecting(
        &mut self,
        player_index: PlayerIndex,
        now: Instant,
        reconnect_grace: Duration,
    ) {
        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
        {
            slot.status = PlayerStatus::Reconnecting;
            slot.runtime_state = PlayerRuntimeState::Reconnecting;
            slot.reconnect_deadline = Some(now + reconnect_grace);
        }
        self.enter_recovery_state();
    }

    fn clear_disconnected_slot(&mut self, player_index: PlayerIndex, is_host: bool) {
        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
        {
            slot.reconnect_deadline = None;
            slot.runtime_state = PlayerRuntimeState::Disconnected;
            slot.status = if is_host {
                PlayerStatus::Disconnected
            } else {
                slot.subject_key = None;
                slot.resume_token_hash = None;
                PlayerStatus::Empty
            };
        }
        self.reset_sync_state();

        if is_host {
            self.status = RoomStatus::Closed;
        }
    }

    fn enter_recovery_state(&mut self) {
        self.status = RoomStatus::Recovering;
        self.reset_sync_state();
        self.bump_room_epoch();
        self.bump_session_epoch();
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Connected;
                slot.runtime_state = PlayerRuntimeState::Connected;
            });
    }
}

impl From<ClientRuntimeState> for PlayerRuntimeState {
    fn from(state: ClientRuntimeState) -> Self {
        match state {
            ClientRuntimeState::Connected => PlayerRuntimeState::Connected,
            ClientRuntimeState::CheckingCompatibility => PlayerRuntimeState::CheckingCompatibility,
            ClientRuntimeState::Syncing => PlayerRuntimeState::Syncing,
            ClientRuntimeState::Ready => PlayerRuntimeState::Ready,
            ClientRuntimeState::Playing => PlayerRuntimeState::Playing,
            ClientRuntimeState::Pausing => PlayerRuntimeState::Pausing,
            ClientRuntimeState::Paused => PlayerRuntimeState::Paused,
            ClientRuntimeState::Reconnecting => PlayerRuntimeState::Reconnecting,
            ClientRuntimeState::Disconnected => PlayerRuntimeState::Disconnected,
        }
    }
}
