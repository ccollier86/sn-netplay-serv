//! Lobby activity markers used for idle-retention policy.
//!
//! Transport keepalives and token refreshes should not keep a lobby alive, but
//! visible user intent and active gameplay should.

use serde::{Deserialize, Serialize};

/// Meaningful lobby activity reported by a client or inferred by the server.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyActivityKind {
    /// A connected player is actively playing a child game room.
    GameplayActive,
    /// A connected player is preparing or using temporary ROM relay.
    RomRelay,
    /// A connected player is speaking in lobby voice.
    VoiceSpeaking,
}
