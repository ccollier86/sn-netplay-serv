//! Lobby game selection DTOs.
//!
//! The lobby stores a proposed game separately from active gameplay rooms so a
//! lobby can survive game exits and later launch a different title.

use crate::rooms::PlayerIndex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Game proposal supplied by a lobby client.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyGameCandidate {
    /// User-facing game title.
    pub title: String,
    /// ShadowBoy system id, such as `snes` or `genesis`.
    pub system_id: String,
    /// Emulator core id selected for this game.
    pub core_id: String,
    /// Optional content hash used for exact local ROM matching.
    #[serde(default)]
    pub content_sha256: Option<String>,
    /// Optional ROM size for preview and relay-policy checks.
    #[serde(default)]
    pub rom_size_bytes: Option<u64>,
    /// Optional save-state source label, such as `fresh` or `managed`.
    #[serde(default)]
    pub start_state_label: Option<String>,
}

/// Lobby game selection returned to clients.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyGameSelectionView {
    /// Stable proposal id for client diffing.
    pub proposal_id: Uuid,
    /// Player that selected this game.
    pub selected_by_player_index: u8,
    /// Milliseconds since unix epoch when the proposal was recorded.
    pub selected_at_ms: u128,
    /// Proposed game details.
    pub game: LobbyGameCandidate,
}

impl LobbyGameSelectionView {
    /// Creates a new lobby game proposal view.
    pub fn new(game: LobbyGameCandidate, selected_by: PlayerIndex, selected_at_ms: u128) -> Self {
        Self {
            proposal_id: Uuid::new_v4(),
            selected_by_player_index: selected_by.zero_based(),
            selected_at_ms,
            game,
        }
    }
}
