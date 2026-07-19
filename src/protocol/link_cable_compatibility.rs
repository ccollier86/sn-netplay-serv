//! Link-cable compatibility values sent before link mode starts.
//!
//! The server compares these values so clients with different link protocols or
//! runtime profiles do not exchange timing-sensitive cable packets.

use crate::protocol::LinkCableDescriptor;
use serde::{Deserialize, Serialize};

/// Link-cable runtime compatibility details for one client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCableCompatibility {
    /// ShadowBoy netplay protocol version.
    pub protocol_version: u16,
    /// Console family for the virtual cable, such as `gba`.
    pub system_family: String,
    /// Link protocol identifier, such as `gba-link-cable-v1`.
    pub link_protocol: String,
    /// Runtime profile that can exchange link packets with matching clients.
    pub runtime_profile: String,
    /// Hash of BIOS/system data if required by the runtime.
    pub system_data_hash: Option<String>,
}

impl LinkCableCompatibility {
    /// Returns whether this client can join the room described by `link`.
    pub fn matches_descriptor(&self, link: &LinkCableDescriptor) -> bool {
        self.system_family == link.system_family
            && self.link_protocol == link.link_protocol
            && self.runtime_profile == link.runtime_profile
    }

    /// Compares two clients and returns whether their runtime-sensitive fields match.
    pub fn matches_peer(&self, other: &Self) -> bool {
        self.protocol_version == other.protocol_version
            && self.system_family == other.system_family
            && self.link_protocol == other.link_protocol
            && self.runtime_profile == other.runtime_profile
            && self.system_data_hash == other.system_data_hash
    }
}
