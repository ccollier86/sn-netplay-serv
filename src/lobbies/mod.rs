//! Persistent multiplayer lobbies.
//!
//! Lobbies sit above one-off gameplay rooms. They let users invite friends once,
//! chat or use voice, pick games, launch active rooms, then return to the lobby
//! after a game ends.

mod errors;
mod in_memory_lobby_registry;
mod lobby;
mod lobby_capabilities;
mod lobby_game;
mod lobby_player;
mod lobby_registry_trait;
mod lobby_view;

use crate::rooms::PlayerIndex;
use serde::{Deserialize, Serialize};

pub use errors::LobbyError;
pub use in_memory_lobby_registry::InMemoryLobbyRegistry;
pub use lobby::{Lobby, LobbyStatus, MAX_LOBBY_PLAYERS};
pub use lobby_capabilities::{LobbyClientCapabilities, LobbyServerCapabilities};
pub use lobby_game::{LobbyGameCandidate, LobbyGameSelectionView};
pub use lobby_player::{LobbyPlayerRole, LobbyPlayerSlot, LobbyPlayerSlotView, LobbyPlayerStatus};
pub use lobby_registry_trait::LobbyRegistry;
pub use lobby_view::LobbyView;

/// Parameters used to create a lobby.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLobbyParams {
    /// Optional player name shown in lobby UI.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Client feature support.
    #[serde(default = "LobbyClientCapabilities::desktop_default")]
    pub capabilities: LobbyClientCapabilities,
    /// Optional first game selected for the lobby.
    #[serde(default)]
    pub initial_game: Option<LobbyGameCandidate>,
}

/// Parameters used by a player joining a lobby.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinLobbyParams {
    /// Optional player name shown in lobby UI.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Client feature support.
    #[serde(default = "LobbyClientCapabilities::desktop_default")]
    pub capabilities: LobbyClientCapabilities,
}

/// Successful lobby join grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyJoin {
    /// Lobby state after the operation.
    pub lobby: LobbyView,
    /// Assigned player slot.
    pub player_index: PlayerIndex,
    /// Raw resume token sent once to this client.
    pub resume_token: String,
}

#[cfg(test)]
#[path = "lobby_registry_tests.rs"]
mod lobby_registry_tests;
