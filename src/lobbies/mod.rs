//! Persistent multiplayer lobbies.
//!
//! Lobbies sit above one-off gameplay rooms. They let users invite friends once,
//! chat or use voice, pick games, launch active rooms, then return to the lobby
//! after a game ends.

mod errors;
mod in_memory_lobby_registry;
mod lobby;
mod lobby_activity;
mod lobby_capabilities;
mod lobby_chat;
mod lobby_debug_event;
mod lobby_event;
mod lobby_expiration_task;
mod lobby_game;
mod lobby_launch;
mod lobby_player;
mod lobby_registry_trait;
mod lobby_registry_voice_ops;
mod lobby_rom_relay;
mod lobby_view;
mod lobby_voice_ops;
mod stored_lobby;

use crate::protocol::NetplayVoiceDescriptor;
use crate::rooms::PlayerIndex;
use serde::{Deserialize, Serialize};

pub use errors::LobbyError;
pub use in_memory_lobby_registry::InMemoryLobbyRegistry;
pub use lobby::{Lobby, LobbyStatus, MAX_LOBBY_PLAYERS};
pub use lobby_activity::LobbyActivityKind;
pub use lobby_capabilities::{LobbyClientCapabilities, LobbyServerCapabilities};
pub use lobby_chat::LobbyChatMessageView;
pub use lobby_debug_event::{
    LobbyDebugEvent, LobbyDebugEventLog, LobbyDebugEventSink, LobbyRegistrySnapshot,
    NoopLobbyDebugEventSink,
};
pub use lobby_event::LobbyEvent;
pub use lobby_expiration_task::spawn_lobby_expiration_task;
pub use lobby_game::{LobbyGameCandidate, LobbyGameSelectionView};
pub use lobby_launch::{
    LobbyGameLaunchStatus, LobbyGameLaunchView, LobbyGameReadinessStatus, LobbyGameReadinessView,
};
pub use lobby_player::{LobbyPlayerRole, LobbyPlayerSlot, LobbyPlayerSlotView, LobbyPlayerStatus};
pub use lobby_registry_trait::{LobbyEventReceiver, LobbyRegistry};
pub use lobby_rom_relay::{LobbyRomRelayLimits, LobbyRomRelayTransferIntent};
pub use lobby_view::LobbyView;
pub(crate) use stored_lobby::StoredLobby;

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
    /// Optional lobby-scoped voice request.
    #[serde(default)]
    pub voice: Option<NetplayVoiceDescriptor>,
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
    /// Optional player-specific voice grant. Never broadcast this in lobby views.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<crate::rooms::PlayerVoiceJoinGrant>,
}

/// Result returned after refreshing one lobby voice token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyVoiceTokenRefresh {
    /// Current lobby event sequence.
    pub event_seq: u64,
    /// Current lobby epoch.
    pub lobby_epoch: u64,
    /// Fresh player-specific voice grant.
    pub voice: crate::rooms::PlayerVoiceJoinGrant,
}

#[cfg(test)]
#[path = "lobby_registry_tests.rs"]
mod lobby_registry_tests;
#[cfg(test)]
#[path = "lobby_registry_voice_tests.rs"]
mod lobby_registry_voice_tests;
#[cfg(test)]
#[path = "lobby_rom_relay_tests.rs"]
mod lobby_rom_relay_tests;
