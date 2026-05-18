//! Link-cable room descriptor values.
//!
//! These types describe platform-neutral link compatibility. They deliberately
//! avoid Desktop-only paths or process details so Android and future clients can
//! reject incompatible rooms before joining.

use crate::protocol::SessionDescriptorError;
use crate::protocol::descriptor_validation::validate_id;
use serde::{Deserialize, Serialize};

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
    /// Console family for the virtual cable, such as `gba`.
    pub system_family: String,
    /// Stable protocol id, such as `gba-link-cable-v1`.
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

        Ok(())
    }
}
