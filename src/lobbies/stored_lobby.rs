//! Stored lobby wrapper with event broadcasting.
//!
//! The registry owns lookup and locking; this wrapper owns the event channel
//! beside a lobby and exposes small emit helpers.

use crate::lobbies::lobby_debug_event::current_lobby_timestamp_ms;
use crate::lobbies::{
    Lobby, LobbyChatMessageView, LobbyDebugEvent, LobbyDebugEventLog, LobbyEvent,
    LobbyServerCapabilities, LobbyView,
};
use crate::protocol::LobbyFileRelayGrantPair;
use crate::rooms::ConnectionId;
use tokio::sync::broadcast;

const LOBBY_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Lobby plus event channel stored by the in-memory registry.
pub(crate) struct StoredLobby {
    pub(super) lobby: Lobby,
    debug_events: LobbyDebugEventLog,
    events: broadcast::Sender<LobbyEvent>,
    capabilities: LobbyServerCapabilities,
}

impl StoredLobby {
    /// Creates a stored lobby with a bounded event channel.
    pub(super) fn new(lobby: Lobby, capabilities: LobbyServerCapabilities) -> Self {
        let (events, _) = broadcast::channel(LOBBY_EVENT_CHANNEL_CAPACITY);

        Self {
            lobby,
            debug_events: LobbyDebugEventLog::default(),
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

    /// Returns recent sanitized debug events for this lobby.
    pub(super) fn debug_events(&self, limit: usize) -> Vec<LobbyDebugEvent> {
        self.debug_events.tail(limit)
    }

    /// Records a sanitized debug event without broadcasting it to clients.
    pub(super) fn record_debug_event(&mut self, kind: &str, detail: String) -> LobbyDebugEvent {
        let lobby = self.view();
        let event = LobbyDebugEvent {
            timestamp_ms: current_lobby_timestamp_ms(),
            lobby_id: lobby.lobby_id,
            invite_code: lobby.invite_code,
            event_seq: lobby.event_seq,
            lobby_epoch: lobby.lobby_epoch,
            kind: kind.to_string(),
            detail,
        };

        self.debug_events.push(event.clone());
        event
    }

    /// Broadcasts the current lobby view.
    pub(super) fn emit_state_changed(&self) {
        let _ = self.events.send(LobbyEvent::LobbyStateChanged(self.view()));
    }

    /// Broadcasts a chat message.
    pub(super) fn emit_chat_message(&self, chat: LobbyChatMessageView) {
        let _ = self.events.send(LobbyEvent::ChatMessage(chat));
    }

    /// Broadcasts final lobby closure.
    pub(super) fn emit_lobby_closed(&self, reason: String) {
        let _ = self.events.send(LobbyEvent::LobbyClosed {
            lobby: self.view(),
            reason,
        });
    }

    /// Sends private ROM transfer grants to the two involved sockets.
    pub(super) fn emit_rom_transfer_grants(
        &self,
        source: ConnectionId,
        receiver: ConnectionId,
        grants: LobbyFileRelayGrantPair,
    ) {
        let lobby_epoch = self.lobby.lobby_epoch();
        let _ = self.events.send(LobbyEvent::RomTransferUploadGranted {
            source,
            lobby_epoch,
            grant: grants.upload,
        });
        let _ = self.events.send(LobbyEvent::RomTransferDownloadReady {
            receiver,
            lobby_epoch,
            grant: grants.download,
        });
    }

    /// Sends private startup-state transfer grants to the two involved sockets.
    pub(super) fn emit_startup_state_transfer_grants(
        &self,
        source: ConnectionId,
        receiver: ConnectionId,
        grants: LobbyFileRelayGrantPair,
    ) {
        let lobby_epoch = self.lobby.lobby_epoch();
        let _ = self.events.send(LobbyEvent::StartupStateTransferUploadGranted {
            source,
            lobby_epoch,
            grant: grants.upload,
        });
        let _ = self
            .events
            .send(LobbyEvent::StartupStateTransferDownloadReady {
                receiver,
                lobby_epoch,
                grant: grants.download,
            });
    }
}
