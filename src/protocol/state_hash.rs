//! Deterministic netplay state-hash reports.

use crate::protocol::SessionDescriptorError;
use crate::protocol::descriptor_validation::validate_sha256;
use crate::rooms::PlayerIndex;
use serde::{Deserialize, Serialize};

/// Client report for one deterministic emulator state hash.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateHashReport {
    /// Frame whose serialized core state was hashed.
    pub frame: u64,
    /// Lowercase SHA-256 of the serialized deterministic state.
    pub sha256: String,
}

impl StateHashReport {
    /// Validates the hash shape supplied by a client.
    pub fn validate(&self) -> Result<(), SessionDescriptorError> {
        validate_sha256("sha256", &self.sha256)
    }
}

/// Per-player hash entry included in desync diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStateHashView {
    /// Zero-based player index.
    pub player_index: PlayerIndex,
    /// Lowercase SHA-256 reported by that player.
    pub sha256: String,
}

/// Server view emitted when all reported hashes for one frame do not match.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateHashMismatchView {
    /// Frame where the mismatch was detected.
    pub frame: u64,
    /// Reported hashes by player.
    pub hashes: Vec<PlayerStateHashView>,
}
