//! Lobby WebSocket wire messages.
//!
//! These messages are separate from gameplay room messages so lobbies can evolve
//! without affecting Android's existing direct room path.

use crate::lobbies::{
    LobbyChatMessageView, LobbyGameCandidate, LobbyGameReadinessStatus, LobbyView,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client-to-relay lobby WebSocket message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LobbyClientMessage {
    /// Lightweight keepalive.
    Ping,
    /// Host selects or replaces the proposed game.
    SelectGame {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Proposed game details.
        game: LobbyGameCandidate,
    },
    /// Client reports whether it can launch the selected game.
    SetGameReadiness {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal being evaluated.
        proposal_id: Uuid,
        /// Readiness status for the local player.
        status: LobbyGameReadinessStatus,
        /// Optional short reason shown in UI.
        detail: Option<String>,
    },
    /// Host requests that all ready clients launch the selected game.
    LaunchGame {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal to launch.
        proposal_id: Uuid,
    },
    /// Host publishes the direct gameplay room once it is ready.
    PublishGameRoom {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal being launched.
        proposal_id: Uuid,
        /// User-facing gameplay room invite code.
        room_invite_code: String,
    },
    /// Sends a lobby-scoped chat message.
    Chat {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Chat body.
        body: String,
    },
    /// Client intentionally leaves the lobby.
    Leave {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Optional safe reason string for future diagnostics.
        reason: Option<String>,
    },
}

/// Relay-to-client lobby WebSocket message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LobbyServerMessage {
    /// Reply to client ping.
    Pong,
    /// Initial socket join grant.
    LobbyJoined {
        /// Current lobby event sequence.
        event_seq: u64,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Assigned zero-based player index.
        your_player_index: u8,
        /// Opaque token for reclaiming this lobby slot.
        resume_token: String,
        /// Current lobby state.
        lobby: LobbyView,
    },
    /// Lobby state changed.
    LobbyStateChanged {
        /// Current lobby event sequence.
        event_seq: u64,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Current lobby state.
        lobby: LobbyView,
    },
    /// Lobby chat message.
    ChatMessage {
        /// Chat details.
        message: LobbyChatMessageView,
    },
    /// Stable lobby protocol error.
    Error {
        /// Machine-readable error code.
        code: String,
        /// Safe user-facing message.
        message: String,
    },
}
