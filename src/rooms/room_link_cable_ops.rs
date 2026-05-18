//! Link-cable operations for active rooms.
//!
//! This module keeps link compatibility and packet orchestration out of the
//! general room state machine. It mutates only room-domain state and delegates
//! packet validation to `LinkCableRoomState`.

use crate::protocol::{
    LinkCableCompatibility, LinkCableDescriptor, LinkCablePacket, LinkCablePacketLimits,
    NetplaySessionMode,
};
use crate::rooms::{ConnectionId, NetplayRoom, PlayerStatus, RoomError, RoomStatus};

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

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let link = self.link_descriptor()?.clone();

        if let Err(error) =
            self.link_cable_state
                .set_compatibility(player_index, &link, compatibility)
        {
            self.ready_players.remove(&player_index);
            self.status = RoomStatus::CheckingCompatibility;
            self.set_player_status(player_index, PlayerStatus::CompatibilityFailed);
            return Err(error);
        }

        self.ready_players.remove(&player_index);
        self.set_player_status(player_index, PlayerStatus::CheckingCompatibility);
        let connected_players = self.connected_player_indices();

        if !self
            .link_cable_state
            .connected_players_have_compatibility(&connected_players, self.max_players)
        {
            return Ok(());
        }

        if !self
            .link_cable_state
            .connected_players_are_compatible(&connected_players, self.max_players)
        {
            self.status = RoomStatus::CheckingCompatibility;
            self.players
                .iter_mut()
                .filter(|slot| !slot.is_empty())
                .for_each(|slot| slot.status = PlayerStatus::CompatibilityFailed);
            return Err(RoomError::CompatibilityMismatch);
        }

        self.status = RoomStatus::SyncingState;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| slot.status = PlayerStatus::SyncingState);

        Ok(())
    }

    /// Validates and records a link-cable packet from one connection.
    pub fn accept_link_cable_packet(
        &mut self,
        connection_id: ConnectionId,
        packet: &LinkCablePacket,
        limits: LinkCablePacketLimits,
    ) -> Result<(), RoomError> {
        if self.session.mode != NetplaySessionMode::LinkCable || self.status != RoomStatus::Playing
        {
            return Err(RoomError::NotPlaying);
        }

        let owned_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        self.link_cable_state
            .accept_packet(owned_index, packet, limits)
    }

    fn link_descriptor(&self) -> Result<&LinkCableDescriptor, RoomError> {
        if self.session.mode != NetplaySessionMode::LinkCable {
            return Err(RoomError::CompatibilityMismatch);
        }

        self.session
            .link
            .as_ref()
            .ok_or(RoomError::CompatibilityMismatch)
    }
}
