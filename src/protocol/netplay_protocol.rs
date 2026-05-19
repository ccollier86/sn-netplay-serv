//! Netplay protocol version metadata.
//!
//! Desktop and the relay use this small contract before a socket starts moving
//! snapshots or frame input. Version checks stay here so HTTP and room code do
//! not each invent their own rules.

use serde::Serialize;

/// Current relay protocol version.
pub const NETPLAY_PROTOCOL_VERSION: u16 = 4;

/// Oldest Desktop protocol version this relay accepts.
pub const MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION: u16 = 4;

/// Serializable protocol compatibility view returned with room state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplayProtocolView {
    /// Current relay protocol version.
    pub protocol_version: u16,
    /// Oldest Desktop protocol version accepted by this relay.
    pub min_supported_protocol_version: u16,
}

impl Default for NetplayProtocolView {
    fn default() -> Self {
        Self {
            protocol_version: NETPLAY_PROTOCOL_VERSION,
            min_supported_protocol_version: MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION,
        }
    }
}

/// Verifies a Desktop protocol version against this relay.
pub fn validate_client_protocol_version(version: u16) -> Result<(), ProtocolVersionError> {
    if (MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION..=NETPLAY_PROTOCOL_VERSION).contains(&version) {
        Ok(())
    } else {
        Err(ProtocolVersionError { version })
    }
}

/// Unsupported Desktop protocol version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("unsupported desktop netplay protocol version {version}")]
pub struct ProtocolVersionError {
    /// Version supplied by Desktop.
    pub version: u16,
}
