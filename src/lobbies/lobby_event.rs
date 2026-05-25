//! Lobby domain events.
//!
//! Events let WebSocket transports broadcast lobby changes without putting
//! socket concepts into the lobby state machine.

use crate::lobbies::{LobbyChatMessageView, LobbyView};

/// Event emitted after lobby state or chat changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LobbyEvent {
    /// Serializable lobby state should be broadcast to subscribers.
    LobbyStateChanged(LobbyView),
    /// Lobby chat message should be broadcast to subscribers.
    ChatMessage(LobbyChatMessageView),
}
