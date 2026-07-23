//! Link-cable compatibility values sent before link mode starts.
//!
//! The server compares these values so clients with different link protocols or
//! runtime profiles do not exchange timing-sensitive cable packets.

use crate::protocol::descriptor_validation::validate_id;
use crate::protocol::{LinkCableDescriptor, LinkCableMode, SessionDescriptorError};
use serde::{Deserialize, Serialize};

/// Link-cable runtime compatibility details for one client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCableCompatibility {
    /// ShadowBoy netplay protocol version.
    pub protocol_version: u16,
    /// Console family for the virtual cable, such as `gba`.
    pub system_family: String,
    /// Frozen link protocol identifier.
    pub link_protocol: String,
    /// Runtime profile that can exchange link packets with matching clients.
    pub runtime_profile: String,
    /// Exact mGBA build containing the reviewed link adapters.
    pub core_build_id: String,
    /// Link modes implemented by this endpoint.
    pub supported_modes: Vec<LinkCableMode>,
}

impl LinkCableCompatibility {
    /// Validates bounded, platform-neutral compatibility fields.
    pub fn validate(&self) -> Result<(), SessionDescriptorError> {
        validate_id("linkCompatibility.systemFamily", &self.system_family)?;
        validate_id("linkCompatibility.linkProtocol", &self.link_protocol)?;
        validate_id("linkCompatibility.runtimeProfile", &self.runtime_profile)?;
        validate_id("linkCompatibility.coreBuildId", &self.core_build_id)?;

        if self.supported_modes.len() != 1 {
            return Err(SessionDescriptorError {
                field: "linkCompatibility.supportedModes",
            });
        }

        Ok(())
    }

    /// Returns whether this client can join the room described by `link`.
    pub fn matches_descriptor(&self, link: &LinkCableDescriptor) -> bool {
        self.validate().is_ok()
            && self.system_family == link.system_family
            && self.link_protocol == link.link_protocol
            && self.runtime_profile == link.runtime_profile
            && link
                .required_mode()
                .is_some_and(|mode| self.supported_modes.as_slice() == [mode])
    }

    /// Compares two clients and returns whether their runtime-sensitive fields match.
    pub fn matches_peer(&self, other: &Self) -> bool {
        self.protocol_version == other.protocol_version
            && self.system_family == other.system_family
            && self.link_protocol == other.link_protocol
            && self.runtime_profile == other.runtime_profile
            && self.core_build_id == other.core_build_id
            && self.supported_modes == other.supported_modes
    }
}

#[cfg(test)]
mod tests {
    use super::LinkCableCompatibility;
    use crate::protocol::{LinkCableDescriptor, LinkCableMode, LinkCableTransport};

    #[test]
    fn matches_only_the_descriptor_mode_and_exact_peer_build() {
        let descriptor = LinkCableDescriptor {
            system_family: "gba".to_string(),
            link_protocol: "gba-sio-multi-v1".to_string(),
            runtime_profile: "mgba-link-runtime-v1".to_string(),
            max_players: 2,
            transport: LinkCableTransport::Relay,
        };
        let baseline = compatibility("android-mgba-0.10.5-sb1", LinkCableMode::Multi);
        let other_build = compatibility("android-mgba-0.10.5-sb2", LinkCableMode::Multi);
        let wrong_mode = compatibility("android-mgba-0.10.5-sb1", LinkCableMode::Serial);

        assert!(baseline.matches_descriptor(&descriptor));
        assert!(!wrong_mode.matches_descriptor(&descriptor));
        assert!(baseline.matches_peer(&baseline));
        assert!(!baseline.matches_peer(&other_build));
    }

    fn compatibility(core_build_id: &str, mode: LinkCableMode) -> LinkCableCompatibility {
        LinkCableCompatibility {
            protocol_version: crate::protocol::NETPLAY_PROTOCOL_VERSION,
            system_family: "gba".to_string(),
            link_protocol: "gba-sio-multi-v1".to_string(),
            runtime_profile: "mgba-link-runtime-v1".to_string(),
            core_build_id: core_build_id.to_string(),
            supported_modes: vec![mode],
        }
    }
}
