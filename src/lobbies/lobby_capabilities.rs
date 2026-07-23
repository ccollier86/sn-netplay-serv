//! Lobby capability contract.
//!
//! Capabilities let Desktop adopt lobbies first while Android keeps the current
//! direct game-room flow until it is ready for lobby features.

use serde::{Deserialize, Serialize};

use crate::lobbies::LobbyLinkCableClientCapabilities;

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
    /// Client can receive the richer return-to-lobby event tag.
    #[serde(default)]
    pub supports_lobby_returned_event: bool,
    /// Client can receive the `playing` launch sub-status after gameplay starts.
    #[serde(default)]
    pub supports_lobby_gameplay_started: bool,
    /// Client can receive the terminal `playerRemoved` event.
    #[serde(default)]
    pub supports_lobby_player_removed_event: bool,
    /// Optional normal-lobby link-cable contract supported by this client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_cable: Option<LobbyLinkCableClientCapabilities>,
}

impl LobbyClientCapabilities {
    /// Returns the default Desktop lobby capability set for early clients.
    pub fn desktop_default() -> Self {
        Self {
            supports_lobby: true,
            supports_temporary_session_rom_relay: false,
            supports_lobby_voice: true,
            supports_multi_game_lobby: true,
            supports_lobby_returned_event: false,
            supports_lobby_gameplay_started: false,
            supports_lobby_player_removed_event: false,
            link_cable: None,
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
    /// Relay can emit the richer return-to-lobby event tag to capable clients.
    pub supports_lobby_returned_event: bool,
    /// Relay can track active gameplay after the scheduled start barrier releases.
    pub supports_lobby_gameplay_started: bool,
    /// Relay supports host-authorized removal of occupied guest slots.
    pub supports_lobby_player_removal: bool,
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
            supports_lobby_returned_event: true,
            supports_lobby_gameplay_started: true,
            supports_lobby_player_removal: true,
            max_players,
        }
    }
}
