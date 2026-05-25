//! Stored lobby wrapper with event broadcasting.
//!
//! The registry owns lookup and locking; this wrapper owns the event channel
//! beside a lobby and exposes small emit helpers.

use crate::lobbies::{Lobby, LobbyChatMessageView, LobbyEvent, LobbyServerCapabilities, LobbyView};
use tokio::sync::broadcast;

const LOBBY_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Lobby plus event channel stored by the in-memory registry.
pub(crate) struct StoredLobby {
    pub(super) lobby: Lobby,
    events: broadcast::Sender<LobbyEvent>,
    capabilities: LobbyServerCapabilities,
}

impl StoredLobby {
    /// Creates a stored lobby with a bounded event channel.
    pub(super) fn new(lobby: Lobby, capabilities: LobbyServerCapabilities) -> Self {
        let (events, _) = broadcast::channel(LOBBY_EVENT_CHANNEL_CAPACITY);

        Self {
            lobby,
            events,
            capabilities,
        }
    }

    /// Subscribes to lobby events.
    pub(super) fn subscribe(&self) -> broadcast::Receiver<LobbyEvent> {
        self.events.subscribe()
    }

    /// Returns the current lobby view.
    pub(super) fn view(&self) -> LobbyView {
        self.lobby.view(self.capabilities.clone())
    }

    /// Broadcasts the current lobby view.
    pub(super) fn emit_state_changed(&self) {
        let _ = self.events.send(LobbyEvent::LobbyStateChanged(self.view()));
    }

    /// Broadcasts a chat message.
    pub(super) fn emit_chat_message(&self, chat: LobbyChatMessageView) {
        let _ = self.events.send(LobbyEvent::ChatMessage(chat));
    }
}
