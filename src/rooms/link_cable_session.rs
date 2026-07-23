//! Link-cable gameplay provider.
//!
//! This wrapper keeps provisional link state behind the provider boundary. It
//! intentionally does not reuse controller input, snapshots, frame clocks, or
//! coordinated pause state.

use super::link_cable_room_state::LinkCableRoomState;
use crate::protocol::{
    LinkCableCompatibility, LinkCableDescriptor, LinkCablePacket, LinkCablePacketLimits,
};
use crate::rooms::{PlayerIndex, RoomError};

/// Runtime state owned only by link-cable rooms.
#[derive(Clone, Debug, Default)]
pub(crate) struct LinkCableSession {
    state: LinkCableRoomState,
}

impl LinkCableSession {
    /// Clears compatibility and packet ordering before a new join cycle.
    pub(crate) fn reset(&mut self) {
        self.state.reset();
    }

    /// Stores one player's provisional link compatibility.
    pub(crate) fn set_compatibility(
        &mut self,
        player_index: PlayerIndex,
        protocol_version: u16,
        link: &LinkCableDescriptor,
        compatibility: LinkCableCompatibility,
    ) -> Result<(), RoomError> {
        self.state
            .set_compatibility(player_index, protocol_version, link, compatibility)
    }

    /// Returns whether every connected player supplied link compatibility.
    pub(crate) fn connected_players_have_compatibility(
        &self,
        connected_players: &[PlayerIndex],
        max_players: u8,
    ) -> bool {
        self.state
            .connected_players_have_compatibility(connected_players, max_players)
    }

    /// Returns whether all connected link compatibility values agree.
    pub(crate) fn connected_players_are_compatible(
        &self,
        connected_players: &[PlayerIndex],
        max_players: u8,
    ) -> bool {
        self.state
            .connected_players_are_compatible(connected_players, max_players)
    }

    /// Validates and records one provisional link packet.
    pub(crate) fn accept_packet(
        &mut self,
        owned_index: PlayerIndex,
        packet: &LinkCablePacket,
        limits: LinkCablePacketLimits,
    ) -> Result<(), RoomError> {
        self.state.accept_packet(owned_index, packet, limits)
    }
}
