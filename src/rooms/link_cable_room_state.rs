//! Link-cable-specific room state.
//!
//! This module owns compatibility fingerprints and packet sequence validation
//! for virtual link-cable rooms. It does not assign slots, mutate room status,
//! or know about HTTP/WebSocket transport.

use crate::protocol::{
    LinkCableCompatibility, LinkCableDescriptor, LinkCablePacket, LinkCablePacketLimits,
};
use crate::rooms::{PlayerIndex, RoomError};
use std::collections::HashMap;

/// Runtime state for one link-cable room.
#[derive(Clone, Debug, Default)]
pub struct LinkCableRoomState {
    compatibility: HashMap<PlayerIndex, LinkCableCompatibility>,
    last_sequences: HashMap<PlayerIndex, u64>,
}

impl LinkCableRoomState {
    /// Clears compatibility and packet-order state before a new join/sync cycle.
    pub fn reset(&mut self) {
        self.compatibility.clear();
        self.last_sequences.clear();
    }

    /// Stores one player's link compatibility and validates it against the room.
    pub fn set_compatibility(
        &mut self,
        player_index: PlayerIndex,
        protocol_version: u16,
        link: &LinkCableDescriptor,
        compatibility: LinkCableCompatibility,
    ) -> Result<(), RoomError> {
        if compatibility.protocol_version != protocol_version
            || !compatibility.matches_descriptor(link)
        {
            self.compatibility.remove(&player_index);
            return Err(RoomError::CompatibilityMismatch);
        }

        self.compatibility.insert(player_index, compatibility);
        Ok(())
    }

    /// Returns whether every connected player has compatible link metadata.
    pub fn connected_players_have_compatibility(
        &self,
        connected_players: &[PlayerIndex],
        max_players: u8,
    ) -> bool {
        connected_players.len() == usize::from(max_players)
            && connected_players
                .iter()
                .all(|player_index| self.compatibility.contains_key(player_index))
    }

    /// Returns whether every connected player has matching link metadata.
    pub fn connected_players_are_compatible(
        &self,
        connected_players: &[PlayerIndex],
        max_players: u8,
    ) -> bool {
        self.connected_players_have_compatibility(connected_players, max_players)
            && self.compatibility_values_match()
    }

    /// Validates one link packet from its owning player.
    pub fn accept_packet(
        &mut self,
        owned_index: PlayerIndex,
        packet: &LinkCablePacket,
        limits: LinkCablePacketLimits,
    ) -> Result<(), RoomError> {
        if owned_index != packet.player_index {
            return Err(RoomError::SlotSpoofing(packet.player_index));
        }

        packet
            .validate(limits)
            .map_err(|_| RoomError::LinkPacketInvalid)?;

        if let Some(last_sequence) = self.last_sequences.get(&packet.player_index)
            && packet.sequence <= *last_sequence
        {
            return Err(RoomError::OutOfOrderLinkPacket);
        }

        self.last_sequences
            .insert(packet.player_index, packet.sequence);

        Ok(())
    }

    fn compatibility_values_match(&self) -> bool {
        let mut values = self.compatibility.values();
        let Some(baseline) = values.next() else {
            return false;
        };

        values.all(|candidate| baseline.matches_peer(candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::LinkCableRoomState;
    use crate::protocol::{LinkCableDescriptor, LinkCablePacket, LinkCablePacketLimits};
    use crate::rooms::{PlayerIndex, RoomError};

    #[test]
    fn compatible_players_match() {
        let mut state = LinkCableRoomState::default();
        let link = link_descriptor();

        state
            .set_compatibility(
                PlayerIndex::ONE,
                crate::protocol::NETPLAY_PROTOCOL_VERSION,
                &link,
                compatibility(None),
            )
            .expect("p1 compatibility");
        state
            .set_compatibility(
                PlayerIndex::TWO,
                crate::protocol::NETPLAY_PROTOCOL_VERSION,
                &link,
                compatibility(None),
            )
            .expect("p2 compatibility");

        assert!(state.connected_players_are_compatible(&[PlayerIndex::ONE, PlayerIndex::TWO], 2));
    }

    #[test]
    fn mismatched_system_data_does_not_match() {
        let mut state = LinkCableRoomState::default();
        let link = link_descriptor();

        state
            .set_compatibility(
                PlayerIndex::ONE,
                crate::protocol::NETPLAY_PROTOCOL_VERSION,
                &link,
                compatibility(Some("a")),
            )
            .expect("p1 compatibility");
        state
            .set_compatibility(
                PlayerIndex::TWO,
                crate::protocol::NETPLAY_PROTOCOL_VERSION,
                &link,
                compatibility(Some("b")),
            )
            .expect("p2 compatibility");

        assert!(!state.connected_players_are_compatible(&[PlayerIndex::ONE, PlayerIndex::TWO], 2));
    }

    #[test]
    fn packet_sequence_must_increase() {
        let mut state = LinkCableRoomState::default();
        let packet = packet(1);

        state
            .accept_packet(PlayerIndex::ONE, &packet, LinkCablePacketLimits::default())
            .expect("first packet");

        assert_eq!(
            state.accept_packet(PlayerIndex::ONE, &packet, LinkCablePacketLimits::default()),
            Err(RoomError::OutOfOrderLinkPacket)
        );
    }

    fn link_descriptor() -> LinkCableDescriptor {
        LinkCableDescriptor {
            system_family: "gba".to_string(),
            link_protocol: "gba-link-cable-v1".to_string(),
            runtime_profile: "mgba-link-runtime-v1".to_string(),
            max_players: 2,
            transport: Default::default(),
        }
    }

    fn compatibility(system_data_hash: Option<&str>) -> crate::protocol::LinkCableCompatibility {
        crate::protocol::LinkCableCompatibility {
            protocol_version: crate::protocol::NETPLAY_PROTOCOL_VERSION,
            system_family: "gba".to_string(),
            link_protocol: "gba-link-cable-v1".to_string(),
            runtime_profile: "mgba-link-runtime-v1".to_string(),
            system_data_hash: system_data_hash.map(str::to_string),
        }
    }

    fn packet(sequence: u64) -> LinkCablePacket {
        LinkCablePacket {
            player_index: PlayerIndex::ONE,
            sequence,
            emulated_time: sequence * 10,
            payload: vec![1, 2],
        }
    }
}
