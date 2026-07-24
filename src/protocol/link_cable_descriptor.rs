//! Link-cable room descriptor values.
//!
//! These types describe platform-neutral link compatibility. They deliberately
//! avoid Desktop-only paths or process details so Android and future clients can
//! reject incompatible rooms before joining.

use crate::protocol::SessionDescriptorError;
use crate::protocol::descriptor_validation::validate_id;
use serde::{Deserialize, Serialize};

/// Explicit control-plane contract required before completed link traffic is admitted.
pub const LINK_CABLE_CONTRACT_VERSION: u16 = 1;

/// Emulator link mode implemented by one frozen wire-protocol namespace.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkCableMode {
    /// GB/GBC two-device serial exchange.
    Serial,
    /// GBA two-device multiplayer SIO exchange.
    Multi,
}

/// Link-cable transport expected by the clients in this room.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkCableTransport {
    /// Link packets are relayed through the ShadowBoy netplay server.
    #[default]
    Relay,
}

/// Platform-neutral link-cable compatibility descriptor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCableDescriptor {
    /// Console family for the virtual cable: `gb` or `gba`.
    pub system_family: String,
    /// Negotiated event protocol, such as `gb-serial-v1` or `gba-sio-multi-v2`.
    pub link_protocol: String,
    /// Runtime compatibility key shared by clients that can exchange packets.
    pub runtime_profile: String,
    /// Maximum player count requested by the host.
    pub max_players: u8,
    /// Link packet transport used by clients.
    #[serde(default)]
    pub transport: LinkCableTransport,
}

impl LinkCableDescriptor {
    /// Validates link-cable compatibility metadata.
    pub fn validate(&self) -> Result<(), SessionDescriptorError> {
        validate_id("link.systemFamily", &self.system_family)?;
        validate_id("link.linkProtocol", &self.link_protocol)?;
        validate_id("link.runtimeProfile", &self.runtime_profile)?;

        if self.max_players != 2 {
            return Err(SessionDescriptorError {
                field: "link.maxPlayers",
            });
        }

        self.required_mode().ok_or(SessionDescriptorError {
            field: if matches!(self.system_family.as_str(), "gb" | "gba") {
                "link.linkProtocol"
            } else {
                "link.systemFamily"
            },
        })?;

        Ok(())
    }

    /// Returns the one runtime mode required by this frozen protocol pair.
    pub fn required_mode(&self) -> Option<LinkCableMode> {
        match (self.system_family.as_str(), self.link_protocol.as_str()) {
            ("gb", "gb-serial-v1") => Some(LinkCableMode::Serial),
            ("gba", "gba-sio-multi-v1" | "gba-sio-multi-v2") => Some(LinkCableMode::Multi),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LinkCableDescriptor, LinkCableMode, LinkCableTransport};
    use crate::protocol::SessionDescriptorError;

    #[test]
    fn accepts_only_the_frozen_gb_and_gba_protocol_pairs() {
        assert_eq!(
            descriptor("gb", "gb-serial-v1").required_mode(),
            Some(LinkCableMode::Serial)
        );
        assert_eq!(
            descriptor("gba", "gba-sio-multi-v1").required_mode(),
            Some(LinkCableMode::Multi)
        );
        assert_eq!(
            descriptor("gba", "gba-sio-multi-v2").required_mode(),
            Some(LinkCableMode::Multi)
        );
        assert!(descriptor("gb", "gb-serial-v1").validate().is_ok());
        assert!(descriptor("gba", "gba-sio-multi-v1").validate().is_ok());
        assert!(descriptor("gba", "gba-sio-multi-v2").validate().is_ok());
    }

    #[test]
    fn rejects_the_provisional_protocol_and_cross_family_pairs() {
        assert_eq!(
            descriptor("gba", "gba-link-cable-v1").validate(),
            Err(SessionDescriptorError {
                field: "link.linkProtocol"
            })
        );
        assert_eq!(
            descriptor("gb", "gba-sio-multi-v1").validate(),
            Err(SessionDescriptorError {
                field: "link.linkProtocol"
            })
        );
    }

    fn descriptor(system_family: &str, link_protocol: &str) -> LinkCableDescriptor {
        LinkCableDescriptor {
            system_family: system_family.to_string(),
            link_protocol: link_protocol.to_string(),
            runtime_profile: "mgba-link-runtime-v1".to_string(),
            max_players: 2,
            transport: LinkCableTransport::Relay,
        }
    }
}
