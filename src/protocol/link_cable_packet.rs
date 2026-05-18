//! Virtual link-cable packet payloads.
//!
//! Packet bytes are opaque to the relay. The server validates ownership,
//! monotonic sequence, and size only; emulator-specific interpretation remains
//! inside compatible clients.

use crate::limits::MAX_LINK_CABLE_PACKET_BYTES;
use crate::rooms::PlayerIndex;
use serde::{Deserialize, Serialize};

/// One virtual link-cable packet produced by a client runtime.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCablePacket {
    /// Server-assigned player index that produced the packet.
    pub player_index: PlayerIndex,
    /// Monotonic sender-local sequence number.
    pub sequence: u64,
    /// Sender-local emulated timestamp or cycle counter.
    pub emulated_time: u64,
    /// Opaque runtime packet bytes.
    pub payload: Vec<u8>,
}

impl LinkCablePacket {
    /// Validates packet size against relay limits.
    pub fn validate(&self, limits: LinkCablePacketLimits) -> Result<(), LinkCablePacketError> {
        if self.payload.is_empty() {
            return Err(LinkCablePacketError::EmptyPayload);
        }

        if self.payload.len() > limits.max_payload_bytes {
            return Err(LinkCablePacketError::PayloadTooLarge);
        }

        Ok(())
    }
}

/// Link-cable packet relay limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkCablePacketLimits {
    /// Maximum accepted opaque payload bytes.
    pub max_payload_bytes: usize,
}

impl Default for LinkCablePacketLimits {
    fn default() -> Self {
        Self {
            max_payload_bytes: MAX_LINK_CABLE_PACKET_BYTES,
        }
    }
}

/// Link-cable packet validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LinkCablePacketError {
    /// Link packet had no opaque payload bytes.
    #[error("link packet payload is empty")]
    EmptyPayload,
    /// Link packet exceeded relay size limits.
    #[error("link packet payload is too large")]
    PayloadTooLarge,
}
