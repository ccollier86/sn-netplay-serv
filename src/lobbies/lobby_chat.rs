//! Lobby chat DTOs.
//!
//! Chat is lobby-scoped so it can continue before games, during games, and
//! after players return to the lobby.

use crate::rooms::PlayerIndex;
use serde::Serialize;
use uuid::Uuid;

/// Sanitized lobby chat message broadcast to clients.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyChatMessageView {
    /// Stable message id for client de-duplication.
    pub message_id: Uuid,
    /// Sending player index.
    pub player_index: u8,
    /// Timestamp in milliseconds since unix epoch.
    pub sent_at_ms: u128,
    /// Sanitized body.
    pub body: String,
}

impl LobbyChatMessageView {
    /// Creates a sanitized chat message view.
    pub fn new(player_index: PlayerIndex, body: String, sent_at_ms: u128) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            player_index: player_index.zero_based(),
            sent_at_ms,
            body,
        }
    }
}
