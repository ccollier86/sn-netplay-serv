//! Netplay protocol version metadata.
//!
//! Desktop and the relay use this small contract before a socket starts moving
//! snapshots or frame input. Version checks stay here so HTTP and room code do
//! not each invent their own rules.

use serde::Serialize;

/// Current relay protocol version.
pub const NETPLAY_PROTOCOL_VERSION: u16 = 5;

/// Oldest Desktop protocol version this relay accepts.
pub const MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION: u16 = 4;

/// Legacy production protocol retained during the v5 migration.
pub const LEGACY_NETPLAY_PROTOCOL_VERSION: u16 = 4;

/// Serializable protocol compatibility view returned with room state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplayProtocolView {
    /// Exact protocol selected for this room.
    pub protocol_version: u16,
    /// Oldest protocol version accepted by this relay.
    pub min_supported_protocol_version: u16,
    /// Newest protocol version accepted by this relay.
    pub max_supported_protocol_version: u16,
    /// Explicit exact protocol selected for this room.
    pub room_protocol_version: u16,
}

impl Default for NetplayProtocolView {
    fn default() -> Self {
        Self::for_room(NETPLAY_PROTOCOL_VERSION)
    }
}

impl NetplayProtocolView {
    /// Builds the server compatibility view for one exact room protocol.
    pub fn for_room(room_protocol_version: u16) -> Self {
        Self {
            protocol_version: room_protocol_version,
            min_supported_protocol_version: MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION,
            max_supported_protocol_version: NETPLAY_PROTOCOL_VERSION,
            room_protocol_version,
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

/// Selects the newest relay protocol supported by a client-advertised range.
pub fn negotiate_client_protocol_version(
    minimum: u16,
    maximum: u16,
) -> Result<u16, ProtocolVersionError> {
    if minimum > maximum {
        return Err(ProtocolVersionError { version: maximum });
    }

    let common_minimum = minimum.max(MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION);
    let common_maximum = maximum.min(NETPLAY_PROTOCOL_VERSION);
    if common_minimum > common_maximum {
        return Err(ProtocolVersionError { version: maximum });
    }

    Ok(common_maximum)
}

/// Verifies that a socket repeated the exact protocol selected by its room.
pub fn validate_room_protocol_version(
    version: u16,
    room_protocol_version: u16,
) -> Result<(), ProtocolVersionError> {
    validate_client_protocol_version(version)?;
    if version == room_protocol_version {
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

#[cfg(test)]
mod tests {
    use super::{
        negotiate_client_protocol_version, validate_client_protocol_version,
        validate_room_protocol_version,
    };

    #[test]
    fn negotiates_highest_common_version_and_preserves_legacy_exact_requests() {
        assert_eq!(negotiate_client_protocol_version(4, 5), Ok(5));
        assert_eq!(negotiate_client_protocol_version(4, 4), Ok(4));
        assert!(negotiate_client_protocol_version(5, 4).is_err());
        assert!(negotiate_client_protocol_version(6, 7).is_err());
    }

    #[test]
    fn room_socket_must_repeat_the_selected_exact_version() {
        assert!(validate_client_protocol_version(4).is_ok());
        assert!(validate_client_protocol_version(5).is_ok());
        assert!(validate_room_protocol_version(4, 4).is_ok());
        assert!(validate_room_protocol_version(5, 4).is_err());
    }
}
