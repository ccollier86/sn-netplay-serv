//! Connection lifecycle and recovery operations for active rooms.
//!
//! This module owns slot attachment, disconnection, resume-token recovery, and
//! heartbeat-derived liveness state. It intentionally avoids compatibility,
//! snapshot, and input relay rules.

use crate::auth::VerifiedLicense;
use crate::protocol::{ClientNetworkQualityReport, ClientRuntimeState};
use crate::rooms::{
    ClientTransportCapabilities, ConnectionId, NetplayRoom, PlayerIndex, PlayerRole,
    PlayerRuntimeState, PlayerStatus, ResumeTokenHash, RoomError, RoomStatus,
};
use std::time::{Duration, Instant};

/// Reconnect claim submitted by the room registry after validating transport context.
pub(crate) struct PlayerReconnectRequest<'a> {
    /// Slot index the reconnecting client is reclaiming.
    pub player_index: PlayerIndex,
    /// Server-side hash of the long-lived control resume token.
    pub resume_token_hash: &'a str,
    /// Server-side hash of the rotated control resume token.
    pub next_resume_token_hash: ResumeTokenHash,
    /// Server-side hash of the newly issued input socket token.
    pub input_socket_token_hash: ResumeTokenHash,
    /// Room epoch observed by the reconnecting client.
    pub room_epoch: u64,
    /// Fresh control socket connection id.
    pub connection_id: ConnectionId,
    /// Registry clock timestamp for timeout checks and liveness updates.
    pub now: Instant,
    /// Optional transport features this client can use after reconnecting.
    pub capabilities: ClientTransportCapabilities,
}

impl NetplayRoom {
    /// Adds a guest to the first empty slot and returns their player index.
    pub fn join_guest(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        self.join_guest_with_resume(
            license,
            connection_id,
            String::new(),
            String::new(),
            Instant::now(),
            ClientTransportCapabilities::default(),
        )
    }

    /// Adds a guest and stores the resume-token hash for future reconnects.
    pub fn join_guest_with_resume(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
        resume_token_hash: ResumeTokenHash,
        input_socket_token_hash: ResumeTokenHash,
        now: Instant,
        capabilities: ClientTransportCapabilities,
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
            slot.occupy_guest(
                &license,
                connection_id,
                resume_token_hash,
                input_socket_token_hash,
                now,
                capabilities,
            );
            slot.player_index
        };
        if let Err(error) = self.reset_sync_for_checking_compatibility() {
            self.clear_disconnected_slot(player_index, false);
            return Err(error);
        }
        if self.is_link_cable() {
            if let Err(error) = self.bind_link_cable_connection(player_index, connection_id) {
                let _ = self.close_link_cable_data_plane();
                self.status = RoomStatus::Closed;
                self.clear_disconnected_slot(player_index, false);
                return Err(error);
            }
        }

        Ok(player_index)
    }

    /// Attaches a socket connection to the reserved host slot.
    pub fn attach_host(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        self.attach_host_with_resume(
            license,
            connection_id,
            String::new(),
            String::new(),
            Instant::now(),
            ClientTransportCapabilities::default(),
        )
    }

    /// Attaches a host socket and stores a fresh resume-token hash.
    pub fn attach_host_with_resume(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
        resume_token_hash: ResumeTokenHash,
        input_socket_token_hash: ResumeTokenHash,
        now: Instant,
        capabilities: ClientTransportCapabilities,
    ) -> Result<PlayerIndex, RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let (slot_position, player_index, previous_connection, subject_matches) = self
            .players
            .iter()
            .enumerate()
            .find(|(_, candidate)| candidate.role == PlayerRole::Host)
            .map(|(position, slot)| {
                (
                    position,
                    slot.player_index,
                    slot.connection_id,
                    slot.subject_key
                        .as_deref()
                        .is_some_and(|subject_key| subject_key == license.identity_key()),
                )
            })
            .ok_or(RoomError::UnknownConnection)?;

        if !subject_matches {
            return Err(RoomError::HostSubjectMismatch);
        }

        if let Some(previous_connection) = previous_connection
            && previous_connection != connection_id
        {
            self.invalidate_link_cable_connection_if_present(previous_connection)?;
        }

        {
            let slot = &mut self.players[slot_position];
            slot.connection_id = Some(connection_id);
            slot.input_connection_id = None;
            slot.status = PlayerStatus::Connected;
            slot.runtime_state = PlayerRuntimeState::Connected;
            slot.resume_token_hash = Some(resume_token_hash);
            slot.input_socket_token_hash = Some(input_socket_token_hash);
            slot.input_socket_control_connection_id = Some(connection_id);
            slot.supports_state_file_relay = capabilities.supports_state_file_relay;
            slot.supports_rom_file_relay = capabilities.supports_rom_file_relay;
            slot.supports_scheduled_start = capabilities.supports_scheduled_start;
            slot.supports_clock_sync = capabilities.supports_clock_sync;
            slot.supports_fast_input_relay = capabilities.supports_fast_input_relay;
            slot.reconnect_deadline = None;
            slot.reconnect_room_epoch = None;
            slot.runner_handoff_deadline = None;
            slot.last_seen_at = Some(now);
            slot.latest_local_frame = None;
            slot.latest_local_frame_reported_at = None;
            slot.latest_network_report = None;
            slot.latest_network_reported_at = None;
        }
        self.ready_players.remove(&player_index);
        if self.is_link_cable() {
            self.bind_link_cable_connection(player_index, connection_id)?;
        }

        Ok(player_index)
    }

    /// Marks the connection as disconnected and returns whether the room closed.
    pub fn disconnect(&mut self, connection_id: ConnectionId) -> Result<bool, RoomError> {
        self.disconnect_with_recovery(connection_id, Instant::now(), Duration::from_secs(0))
    }

    /// Ends the room because one player intentionally left.
    pub fn player_exited(
        &mut self,
        connection_id: ConnectionId,
        now: Instant,
    ) -> Result<PlayerIndex, RoomError> {
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        if self.is_link_cable() {
            let _ = self.close_link_cable_data_plane();
        }
        self.status = RoomStatus::Closed;
        self.reset_sync_state();
        self.players
            .iter_mut()
            .filter(|slot| !slot.is_empty())
            .for_each(|slot| {
                slot.connection_id = None;
                slot.input_connection_id = None;
                slot.input_socket_token_hash = None;
                slot.input_socket_control_connection_id = None;
                slot.last_seen_at = Some(now);
                slot.latest_local_frame = None;
                slot.latest_local_frame_reported_at = None;
                slot.latest_network_report = None;
                slot.latest_network_reported_at = None;
                slot.reconnect_deadline = None;
                slot.reconnect_room_epoch = None;
                slot.runner_handoff_deadline = None;
                slot.status = PlayerStatus::Disconnected;
                slot.runtime_state = PlayerRuntimeState::Disconnected;
            });

        Ok(player_index)
    }

    /// Marks the connection disconnected and starts recovery when appropriate.
    pub fn disconnect_with_recovery(
        &mut self,
        connection_id: ConnectionId,
        now: Instant,
        reconnect_grace: Duration,
    ) -> Result<bool, RoomError> {
        if self.preserve_runner_handoff_disconnect(connection_id, now)? {
            self.invalidate_link_cable_connection_if_present(connection_id)?;
            return Ok(false);
        }
        self.invalidate_link_cable_connection_if_present(connection_id)?;

        let slot = self
            .players
            .iter_mut()
            .find(|slot| slot.connection_id == Some(connection_id))
            .ok_or(RoomError::UnknownConnection)?;
        let player_index = slot.player_index;
        let is_host = slot.role == PlayerRole::Host;
        let already_recovering = self.status == RoomStatus::Recovering;
        let recoverable = matches!(
            self.status,
            RoomStatus::StartScheduled
                | RoomStatus::Playing
                | RoomStatus::Paused
                | RoomStatus::RepairingState
                | RoomStatus::Recovering
        );
        let reconnect_room_epoch = slot.reconnect_room_epoch.unwrap_or(if already_recovering {
            self.room_epoch.saturating_sub(1)
        } else {
            self.room_epoch
        });

        slot.connection_id = None;
        slot.input_connection_id = None;
        slot.input_socket_token_hash = None;
        slot.input_socket_control_connection_id = None;
        slot.last_seen_at = Some(now);
        self.compatibility.remove(&player_index);
        self.ready_players.remove(&player_index);
        self.last_input_frames.remove(&player_index);
        self.next_input_frames.remove(&player_index);

        if recoverable {
            self.mark_slot_reconnecting(player_index, now, reconnect_grace, reconnect_room_epoch);
            if !already_recovering {
                self.enter_recovery_state(reconnect_room_epoch);
                if self.status == RoomStatus::Closed {
                    return Ok(true);
                }
            }
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
    pub(crate) fn reconnect_player(
        &mut self,
        request: PlayerReconnectRequest<'_>,
    ) -> Result<(), RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let slot_position = self
            .players
            .iter()
            .position(|slot| slot.player_index == request.player_index)
            .ok_or(RoomError::UnknownConnection)?;
        let slot = &self.players[slot_position];

        if slot.resume_token_hash.as_deref() != Some(request.resume_token_hash) {
            return Err(RoomError::ResumeTokenInvalid);
        }

        let runner_handoff = slot.runner_handoff_deadline.is_some();
        let previous_connection_id = slot.connection_id;
        if !runner_handoff && (slot.connection_id.is_some() || slot.reconnect_deadline.is_none()) {
            return Err(RoomError::ResumeTokenInvalid);
        }

        let accepted_epoch = slot.reconnect_room_epoch.unwrap_or(self.room_epoch);
        if request.room_epoch != accepted_epoch && request.room_epoch != self.room_epoch {
            return Err(RoomError::StaleRoomEpoch);
        }

        let reconnect_expired = slot
            .reconnect_deadline
            .is_some_and(|deadline| request.now >= deadline);
        let runner_handoff_expired = slot
            .runner_handoff_deadline
            .is_some_and(|deadline| request.now >= deadline);
        if reconnect_expired || runner_handoff_expired {
            let slot = &mut self.players[slot_position];
            slot.status = PlayerStatus::RecoveryExpired;
            slot.runtime_state = PlayerRuntimeState::RecoveryExpired;
            return Err(RoomError::RecoveryExpired);
        }

        let link_endpoint_prebound = if self.is_link_cable() && runner_handoff {
            if let Some(previous_connection_id) = previous_connection_id {
                self.replace_link_cable_connection(
                    request.player_index,
                    previous_connection_id,
                    request.connection_id,
                )?;
                true
            } else {
                false
            }
        } else {
            false
        };

        let slot = &mut self.players[slot_position];
        slot.connection_id = Some(request.connection_id);
        slot.input_connection_id = None;
        slot.status = PlayerStatus::Connected;
        slot.runtime_state = PlayerRuntimeState::Reconnecting;
        slot.resume_token_hash = Some(request.next_resume_token_hash);
        slot.input_socket_token_hash = Some(request.input_socket_token_hash);
        slot.input_socket_control_connection_id = Some(request.connection_id);
        slot.supports_state_file_relay = request.capabilities.supports_state_file_relay;
        slot.supports_rom_file_relay = request.capabilities.supports_rom_file_relay;
        slot.supports_scheduled_start = request.capabilities.supports_scheduled_start;
        slot.supports_clock_sync = request.capabilities.supports_clock_sync;
        slot.supports_fast_input_relay = request.capabilities.supports_fast_input_relay;
        slot.last_seen_at = Some(request.now);
        slot.latest_local_frame = None;
        slot.latest_local_frame_reported_at = None;
        slot.latest_network_report = None;
        slot.latest_network_reported_at = None;
        slot.reconnect_deadline = None;
        slot.reconnect_room_epoch = None;
        slot.runner_handoff_deadline = None;
        if !runner_handoff {
            self.restore_checking_compatibility_after_recovery();
        }
        if self.is_link_cable() && !link_endpoint_prebound {
            self.bind_link_cable_connection(request.player_index, request.connection_id)?;
        }

        Ok(())
    }

    /// Records a heartbeat from a connected player.
    pub fn record_heartbeat(
        &mut self,
        connection_id: ConnectionId,
        now: Instant,
        local_frame: Option<u64>,
        network: Option<ClientNetworkQualityReport>,
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
            if let Some(local_frame) = local_frame {
                slot.latest_local_frame = Some(local_frame);
                slot.latest_local_frame_reported_at = Some(now);
            }
            if let Some(network) = network {
                slot.latest_network_report = Some(network);
                slot.latest_network_reported_at = Some(now);
            }
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
        if !matches!(
            self.status,
            RoomStatus::StartScheduled
                | RoomStatus::Playing
                | RoomStatus::Paused
                | RoomStatus::RepairingState
        ) {
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
            .filter_map(|slot| {
                slot.connection_id
                    .map(|connection_id| (slot.player_index, connection_id))
            })
            .collect::<Vec<_>>();

        if stale_players.is_empty() {
            return false;
        }

        for (player_index, connection_id) in stale_players {
            if self
                .invalidate_link_cable_connection_if_present(connection_id)
                .is_err()
            {
                if self.is_link_cable() {
                    let _ = self.close_link_cable_data_plane();
                }
                self.status = RoomStatus::Closed;
                return true;
            }
            self.compatibility.remove(&player_index);
            self.ready_players.remove(&player_index);
            self.last_input_frames.remove(&player_index);
            self.next_input_frames.remove(&player_index);
            if let Some(slot) = self
                .players
                .iter_mut()
                .find(|slot| slot.player_index == player_index)
            {
                slot.connection_id = None;
                slot.input_connection_id = None;
                slot.input_socket_token_hash = None;
                slot.input_socket_control_connection_id = None;
                slot.last_seen_at = Some(now);
                slot.status = PlayerStatus::Reconnecting;
                slot.runtime_state = PlayerRuntimeState::Reconnecting;
                slot.reconnect_deadline = Some(now + reconnect_grace);
                slot.reconnect_room_epoch = Some(self.room_epoch);
                slot.runner_handoff_deadline = None;
            }
        }

        self.enter_recovery_state(self.room_epoch);
        true
    }

    /// Marks heartbeat-stale sockets before they reach recovery timeout.
    pub fn mark_stale_connections(
        &mut self,
        now: Instant,
        heartbeat_stale: Duration,
        heartbeat_disconnect: Duration,
    ) -> bool {
        if !matches!(
            self.status,
            RoomStatus::StartScheduled
                | RoomStatus::Playing
                | RoomStatus::Paused
                | RoomStatus::RepairingState
        ) {
            return false;
        }

        let mut changed = false;
        for slot in self
            .players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
        {
            let Some(last_seen) = slot.last_seen_at else {
                continue;
            };
            let heartbeat_age = now.duration_since(last_seen);

            if heartbeat_age >= heartbeat_stale
                && heartbeat_age < heartbeat_disconnect
                && slot.runtime_state != PlayerRuntimeState::Stale
            {
                slot.runtime_state = PlayerRuntimeState::Stale;
                changed = true;
            }
        }

        changed
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

    fn reset_sync_for_checking_compatibility(&mut self) -> Result<(), RoomError> {
        self.bump_room_epoch();
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }
        self.bump_session_epoch();
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }
        self.restore_checking_compatibility_after_recovery();
        Ok(())
    }

    fn restore_checking_compatibility_after_recovery(&mut self) {
        if self.status == RoomStatus::Closed {
            return;
        }
        self.reset_sync_state();
        self.status = RoomStatus::CheckingCompatibility;
        self.players
            .iter_mut()
            .filter(|slot| !slot.is_empty() && slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Connected;
                slot.runtime_state = PlayerRuntimeState::Connected;
                if slot.runner_handoff_deadline.is_none() {
                    slot.reconnect_room_epoch = None;
                }
            });
    }

    fn mark_slot_reconnecting(
        &mut self,
        player_index: PlayerIndex,
        now: Instant,
        reconnect_grace: Duration,
        reconnect_room_epoch: u64,
    ) {
        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
        {
            slot.status = PlayerStatus::Reconnecting;
            slot.runtime_state = PlayerRuntimeState::Reconnecting;
            slot.reconnect_deadline = Some(now + reconnect_grace);
            slot.reconnect_room_epoch = Some(reconnect_room_epoch);
            slot.runner_handoff_deadline = None;
        }
    }

    fn invalidate_link_cable_connection_if_present(
        &self,
        connection_id: ConnectionId,
    ) -> Result<(), RoomError> {
        if !self.is_link_cable() {
            return Ok(());
        }

        match self.invalidate_link_cable_connection(connection_id) {
            Ok(_) | Err(RoomError::UnknownConnection) => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub(super) fn clear_disconnected_slot(&mut self, player_index: PlayerIndex, is_host: bool) {
        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
        {
            slot.reconnect_deadline = None;
            slot.reconnect_room_epoch = None;
            slot.runner_handoff_deadline = None;
            slot.input_connection_id = None;
            slot.input_socket_token_hash = None;
            slot.input_socket_control_connection_id = None;
            slot.latest_local_frame = None;
            slot.latest_local_frame_reported_at = None;
            slot.latest_network_report = None;
            slot.latest_network_reported_at = None;
            slot.runtime_state = PlayerRuntimeState::Disconnected;
            slot.status = if is_host {
                PlayerStatus::Disconnected
            } else {
                slot.subject_key = None;
                slot.resume_token_hash = None;
                slot.input_socket_token_hash = None;
                PlayerStatus::Empty
            };
        }
        self.reset_sync_state();

        if is_host {
            if self.is_link_cable() {
                let _ = self.close_link_cable_data_plane();
            }
            self.status = RoomStatus::Closed;
        }
    }

    pub(super) fn enter_recovery_state(&mut self, reconnect_room_epoch: u64) {
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
                slot.reconnect_room_epoch
                    .get_or_insert(reconnect_room_epoch);
            });
    }
}
