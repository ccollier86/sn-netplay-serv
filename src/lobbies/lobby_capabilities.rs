//! Lobby capability contract.
//!
//! Capabilities let Desktop adopt lobbies first while Android keeps the current
//! direct game-room flow until it is ready for lobby features.

use serde::{Deserialize, Serialize};

/// Capabilities one connected client reports for lobby behavior.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyClientCapabilities {
    /// Client can join and remain in lobby sessions.
    #[serde(default)]
    pub supports_lobby: bool,
    /// Client can receive temporary ROM data for one live session.
    #[serde(default)]
    pub supports_temporary_session_rom_relay: bool,
    /// Client can join lobby-scoped voice chat.
    #[serde(default)]
    pub supports_lobby_voice: bool,
    /// Client can stay in one lobby while changing games.
    #[serde(default)]
    pub supports_multi_game_lobby: bool,
}

impl LobbyClientCapabilities {
    /// Returns the default Desktop lobby capability set for early clients.
    pub fn desktop_default() -> Self {
        Self {
            supports_lobby: true,
            supports_temporary_session_rom_relay: true,
            supports_lobby_voice: true,
            supports_multi_game_lobby: true,
        }
    }
}

/// Server-supported lobby behavior returned in lobby views.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyServerCapabilities {
    /// Relay supports persistent invite-code lobbies.
    pub supports_lobby: bool,
    /// Relay can coordinate temporary ROM access through a file relay.
    pub supports_temporary_session_rom_relay: bool,
    /// Relay can attach lobby-scoped voice sessions.
    pub supports_lobby_voice: bool,
    /// Relay can keep the lobby alive across multiple games.
    pub supports_multi_game_lobby: bool,
    /// Maximum players accepted by this lobby.
    pub max_players: u8,
}

impl LobbyServerCapabilities {
    /// Creates the current server capability view for a lobby.
    pub fn current(max_players: u8, temporary_session_rom_relay: bool, lobby_voice: bool) -> Self {
        Self {
            supports_lobby: true,
            supports_temporary_session_rom_relay: temporary_session_rom_relay,
            supports_lobby_voice: lobby_voice,
            supports_multi_game_lobby: true,
            max_players,
        }
    }
}
