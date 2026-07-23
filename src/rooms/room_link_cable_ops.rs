//! Link-cable operations for active rooms.
//!
//! This module keeps link compatibility and private data-plane ownership out of
//! the general room state machine.

use crate::protocol::{
    LinkCableCompatibility, LinkCableDescriptor, LinkCablePacket, LinkCablePacketLimits,
};
use crate::rooms::{
    ConnectionId, LinkCableAttachment, LinkCableDataPlaneError, LinkCableDataPlaneHandle,
    LinkCableDataPlaneSnapshot, LinkCableSession, NetplayRoom, PlayerIndex, PlayerRuntimeState,
    PlayerStatus, RoomError, RoomStatus,
};

/// Maps a private link-provider failure onto the stable room-domain vocabulary.
pub(crate) fn map_link_cable_data_plane_error(error: LinkCableDataPlaneError) -> RoomError {
    LinkCableSession::map_data_plane_error(error)
}

impl NetplayRoom {
    /// Stores link-cable compatibility details for a connected player.
    pub fn set_link_cable_compatibility_for_connection(
        &mut self,
        connection_id: ConnectionId,
        compatibility: LinkCableCompatibility,
    ) -> Result<(), RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        if self.status == RoomStatus::Playing {
            return Err(RoomError::RoomNotReady);
        }

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let link = self.link_descriptor()?.clone();
        let protocol_version = self.protocol_version();

        if let Err(error) = self.link_cable_session_mut()?.set_compatibility(
            player_index,
            protocol_version,
            &link,
            compatibility,
        ) {
            self.ready_players.remove(&player_index);
            self.status = RoomStatus::CheckingCompatibility;
            self.set_player_status(player_index, PlayerStatus::CompatibilityFailed);
            return Err(error);
        }

        self.ready_players.remove(&player_index);
        self.set_player_status(player_index, PlayerStatus::CheckingCompatibility);
        let connected_players = self.connected_player_indices();

        if !self
            .link_cable_session()?
            .connected_players_have_compatibility(&connected_players, self.max_players)
        {
            return Ok(());
        }

        if !self
            .link_cable_session()?
            .connected_players_are_compatible(&connected_players, self.max_players)
        {
            self.status = RoomStatus::CheckingCompatibility;
            self.players
                .iter_mut()
                .filter(|slot| !slot.is_empty())
                .for_each(|slot| {
                    slot.status = PlayerStatus::CompatibilityFailed;
                    slot.runtime_state = PlayerRuntimeState::Connected;
                });
            return Err(RoomError::CompatibilityMismatch);
        }

        self.status = RoomStatus::SyncingState;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::SyncingState;
                slot.runtime_state = PlayerRuntimeState::Syncing;
            });

        Ok(())
    }

    /// Returns a private data-plane route after authenticating a playing slot.
    ///
    /// Registry callers clone this handle under their read lock and invoke
    /// `relay` only after releasing the registry lock.
    pub(crate) fn link_cable_data_plane_handle_for_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Result<(LinkCableDataPlaneHandle, PlayerIndex), RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }
        if !self.is_link_cable() || self.status != RoomStatus::Playing {
            return Err(RoomError::NotPlaying);
        }

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        Ok((self.link_cable_session()?.data_plane_handle(), player_index))
    }

    /// Binds a current room connection to one exact link endpoint.
    pub(crate) fn bind_link_cable_connection(
        &self,
        player_index: PlayerIndex,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, RoomError> {
        let owned_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        if owned_index != player_index {
            return Err(RoomError::SlotSpoofing(player_index));
        }

        self.link_cable_session()?
            .bind_connection(player_index, connection_id)
    }

    /// Atomically replaces a currently owned connection during runner handoff.
    pub(crate) fn replace_link_cable_connection(
        &self,
        player_index: PlayerIndex,
        previous_connection_id: ConnectionId,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, RoomError> {
        let owned_index = self
            .player_index_for_connection(previous_connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        if owned_index != player_index {
            return Err(RoomError::SlotSpoofing(player_index));
        }

        self.link_cable_session()?.replace_connection(
            player_index,
            previous_connection_id,
            connection_id,
        )
    }

    /// Claims the one targeted receiver for a bound current connection.
    pub(crate) fn claim_link_cable_receiver(
        &self,
        connection_id: ConnectionId,
    ) -> Result<Option<LinkCableAttachment>, RoomError> {
        if !self.is_link_cable() {
            return Ok(None);
        }
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let owned_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let attachment = self.link_cable_session()?.claim_receiver(connection_id)?;
        if attachment.snapshot.local_slot != owned_index {
            let _ = self
                .link_cable_session()?
                .invalidate_connection(connection_id);
            return Err(RoomError::SlotSpoofing(attachment.snapshot.local_slot));
        }

        Ok(Some(attachment))
    }

    /// Returns link-only lifecycle context for a current control connection.
    ///
    /// The snapshot contains no packet bytes, connection identifiers, invite
    /// codes, license subjects, or bearer tokens.
    pub(crate) fn link_cable_diagnostic_snapshot_for_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Result<Option<LinkCableDataPlaneSnapshot>, RoomError> {
        if !self.is_link_cable() {
            return Ok(None);
        }
        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        self.link_cable_session()?
            .diagnostic_snapshot(player_index)
            .map(Some)
    }

    /// Invalidates one exact link endpoint connection.
    ///
    /// This deliberately does not consult player slots because lifecycle code
    /// may call it immediately after clearing the room connection.
    pub(crate) fn invalidate_link_cable_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, RoomError> {
        self.link_cable_session()?
            .invalidate_connection(connection_id)
    }

    /// Permanently closes this room's private link provider.
    pub(crate) fn close_link_cable_data_plane(&self) -> Result<(), RoomError> {
        self.link_cable_session()?.close()
    }

    /// Validates and relays a link-cable packet through the bounded data plane.
    ///
    /// This compatibility entry point is retained for direct room callers. The
    /// registry uses `link_cable_data_plane_handle_for_connection` so it never
    /// performs decode or queue work while holding its global lock.
    pub fn accept_link_cable_packet(
        &self,
        connection_id: ConnectionId,
        packet: &LinkCablePacket,
        _limits: LinkCablePacketLimits,
    ) -> Result<(), RoomError> {
        let (handle, owned_index) =
            self.link_cable_data_plane_handle_for_connection(connection_id)?;
        handle
            .relay(
                connection_id,
                owned_index,
                self.room_epoch,
                self.session_epoch,
                packet.clone(),
            )
            .map_err(map_link_cable_data_plane_error)
    }

    fn link_descriptor(&self) -> Result<&LinkCableDescriptor, RoomError> {
        if !self.is_link_cable() {
            return Err(RoomError::CompatibilityMismatch);
        }

        self.session
            .link
            .as_ref()
            .ok_or(RoomError::CompatibilityMismatch)
    }

    fn link_cable_session(&self) -> Result<&LinkCableSession, RoomError> {
        self.gameplay_session
            .link_cable()
            .ok_or(RoomError::CompatibilityMismatch)
    }

    fn link_cable_session_mut(&mut self) -> Result<&mut LinkCableSession, RoomError> {
        self.gameplay_session
            .link_cable_mut()
            .ok_or(RoomError::CompatibilityMismatch)
    }
}
