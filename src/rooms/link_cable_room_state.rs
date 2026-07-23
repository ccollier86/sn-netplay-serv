//! Link-cable-specific room state.
//!
//! This module owns compatibility fingerprints for virtual link-cable rooms.
//! Packet ordering and delivery live exclusively in the room-private bounded
//! data plane.

use crate::protocol::{LinkCableCompatibility, LinkCableDescriptor};
use crate::rooms::{PlayerIndex, RoomError};
use std::collections::HashMap;

/// Runtime state for one link-cable room.
#[derive(Clone, Debug, Default)]
pub struct LinkCableRoomState {
    compatibility: HashMap<PlayerIndex, LinkCableCompatibility>,
}

impl LinkCableRoomState {
    /// Clears compatibility before a new join/sync cycle.
    pub fn reset(&mut self) {
        self.compatibility.clear();
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
    use crate::protocol::{LinkCableDescriptor, LinkCableMode};
    use crate::rooms::PlayerIndex;

    #[test]
    fn compatible_players_match() {
        let mut state = LinkCableRoomState::default();
        let link = link_descriptor();

        state
            .set_compatibility(
                PlayerIndex::ONE,
                crate::protocol::NETPLAY_PROTOCOL_VERSION,
                &link,
                compatibility("android-mgba-0.10.5-sb1"),
            )
            .expect("p1 compatibility");
        state
            .set_compatibility(
                PlayerIndex::TWO,
                crate::protocol::NETPLAY_PROTOCOL_VERSION,
                &link,
                compatibility("android-mgba-0.10.5-sb1"),
            )
            .expect("p2 compatibility");

        assert!(state.connected_players_are_compatible(&[PlayerIndex::ONE, PlayerIndex::TWO], 2));
    }

    #[test]
    fn mismatched_core_build_does_not_match() {
        let mut state = LinkCableRoomState::default();
        let link = link_descriptor();

        state
            .set_compatibility(
                PlayerIndex::ONE,
                crate::protocol::NETPLAY_PROTOCOL_VERSION,
                &link,
                compatibility("android-mgba-0.10.5-sb1"),
            )
            .expect("p1 compatibility");
        state
            .set_compatibility(
                PlayerIndex::TWO,
                crate::protocol::NETPLAY_PROTOCOL_VERSION,
                &link,
                compatibility("android-mgba-0.10.5-sb2"),
            )
            .expect("p2 compatibility");

        assert!(!state.connected_players_are_compatible(&[PlayerIndex::ONE, PlayerIndex::TWO], 2));
    }

    fn link_descriptor() -> LinkCableDescriptor {
        LinkCableDescriptor {
            system_family: "gba".to_string(),
            link_protocol: "gba-sio-multi-v1".to_string(),
            runtime_profile: "mgba-link-runtime-v1".to_string(),
            max_players: 2,
            transport: Default::default(),
        }
    }

    fn compatibility(core_build_id: &str) -> crate::protocol::LinkCableCompatibility {
        crate::protocol::LinkCableCompatibility {
            protocol_version: crate::protocol::NETPLAY_PROTOCOL_VERSION,
            system_family: "gba".to_string(),
            link_protocol: "gba-sio-multi-v1".to_string(),
            runtime_profile: "mgba-link-runtime-v1".to_string(),
            core_build_id: core_build_id.to_string(),
            supported_modes: vec![LinkCableMode::Multi],
        }
    }
}
