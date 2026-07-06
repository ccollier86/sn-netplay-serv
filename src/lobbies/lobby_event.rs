//! Lobby domain events.
//!
//! Events let WebSocket transports broadcast lobby changes without putting
//! socket concepts into the lobby state machine.

use crate::lobbies::{LobbyChatMessageView, LobbyView};
use crate::protocol::LobbyFileRelayGrant;
use crate::rooms::ConnectionId;

/// Event emitted after lobby state or chat changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LobbyEvent {
    /// Serializable lobby state should be broadcast to subscribers.
    LobbyStateChanged(LobbyView),
    /// Lobby chat message should be broadcast to subscribers.
    ChatMessage(LobbyChatMessageView),
    /// Lobby closed intentionally on the server side.
    LobbyClosed {
        /// Final serializable lobby state.
        lobby: LobbyView,
        /// Safe close reason.
        reason: String,
    },
    /// Private ROM upload grant for one lobby socket.
    RomTransferUploadGranted {
        /// Sender lobby socket.
        source: ConnectionId,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Private upload grant.
        grant: LobbyFileRelayGrant,
    },
    /// Private ROM download grant for one lobby socket.
    RomTransferDownloadReady {
        /// Receiver lobby socket.
        receiver: ConnectionId,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Private download grant.
        grant: LobbyFileRelayGrant,
    },
    /// Private startup-state upload grant for one lobby socket.
    StartupStateTransferUploadGranted {
        /// Sender lobby socket.
        source: ConnectionId,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Private upload grant.
        grant: LobbyFileRelayGrant,
    },
    /// Private startup-state download grant for one lobby socket.
    StartupStateTransferDownloadReady {
        /// Receiver lobby socket.
        receiver: ConnectionId,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Private download grant.
        grant: LobbyFileRelayGrant,
    },
}
