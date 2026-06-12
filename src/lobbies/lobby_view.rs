//! Serializable lobby views.
//!
//! Views expose enough state for UI and SDK state machines without leaking raw
//! resume tokens or internal authenticated subject keys.

use crate::lobbies::{
    LobbyGameLaunchView, LobbyGameReadinessView, LobbyGameSelectionView, LobbyPlayerSlotView,
    LobbyPlayerStatus, LobbyServerCapabilities, LobbyStatus, LobbyVisibility,
};
use crate::rooms::{RoomId, RoomVoiceView};
use serde::Serialize;

const PUBLIC_LOBBY_PLAYER_CAPACITY: u8 = 2;

/// Current lobby state returned by REST and future lobby WebSocket messages.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyView {
    /// Stable lobby id.
    pub lobby_id: RoomId,
    /// Monotonic event sequence for lobby state changes.
    pub event_seq: u64,
    /// Epoch that changes when lobby membership or selected game changes.
    pub lobby_epoch: u64,
    /// User-facing invite code.
    pub invite_code: String,
    /// Creation timestamp in milliseconds since unix epoch.
    pub created_at_ms: u128,
    /// Last state-change timestamp in milliseconds since unix epoch.
    pub updated_at_ms: u128,
    /// Last user/game activity timestamp used by idle cleanup.
    pub last_meaningful_activity_at_ms: u128,
    /// Current lobby lifecycle status.
    pub status: LobbyStatus,
    /// Discovery visibility for this lobby.
    pub visibility: LobbyVisibility,
    /// Server capability flags for this lobby.
    pub capabilities: LobbyServerCapabilities,
    /// Current player slots in display order.
    pub players: Vec<LobbyPlayerSlotView>,
    /// Selected game proposal, if one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_game: Option<LobbyGameSelectionView>,
    /// Player readiness for the selected game.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub game_readiness: Vec<LobbyGameReadinessView>,
    /// Host launch signal for the selected game.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_launch: Option<LobbyGameLaunchView>,
    /// Lobby-scoped voice room metadata safe to broadcast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<RoomVoiceView>,
}

/// Publicly listable lobby summary. This deliberately exposes less than `LobbyView`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicLobbySummary {
    /// Stable lobby id for UI diffing.
    pub lobby_id: RoomId,
    /// User-facing invite code used when the user clicks Join.
    pub invite_code: String,
    /// Public discovery visibility.
    pub visibility: LobbyVisibility,
    /// Current public lobby status.
    pub status: PublicLobbyStatus,
    /// Host display name, or a safe fallback.
    pub hosted_by: String,
    /// Occupied public gameplay slots.
    pub player_count: u8,
    /// Current public gameplay capacity.
    pub max_players: u8,
    /// Available public gameplay slots.
    pub open_slots: u8,
    /// Creation timestamp in milliseconds since unix epoch.
    pub created_at_ms: u128,
    /// Last state-change timestamp in milliseconds since unix epoch.
    pub updated_at_ms: u128,
    /// Selected game preview, if one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_game: Option<PublicLobbyGameSummary>,
}

/// Public status used by the lobby browser.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PublicLobbyStatus {
    /// Lobby is open but no game is selected.
    Open,
    /// Lobby has a selected game that can be previewed before joining.
    GameSelected,
}

/// Selected game fields safe for public lobby browsing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicLobbyGameSummary {
    /// User-facing game title.
    pub title: String,
    /// ShadowBoy system id.
    pub system_id: String,
    /// Core id selected by the host.
    pub core_id: String,
    /// Optional save-state source label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_state_label: Option<String>,
}

impl LobbyView {
    /// Returns a public summary only while this lobby is publicly joinable.
    pub fn public_summary(&self) -> Option<PublicLobbySummary> {
        if self.visibility != LobbyVisibility::Public {
            return None;
        }

        let status = match self.status {
            LobbyStatus::Open => PublicLobbyStatus::Open,
            LobbyStatus::GameSelected => PublicLobbyStatus::GameSelected,
            LobbyStatus::InGame | LobbyStatus::Closed => return None,
        };
        let player_count = self.public_player_count();

        if player_count >= PUBLIC_LOBBY_PLAYER_CAPACITY || !self.host_is_connected() {
            return None;
        }

        Some(PublicLobbySummary {
            lobby_id: self.lobby_id,
            invite_code: self.invite_code.clone(),
            visibility: self.visibility,
            status,
            hosted_by: self.hosted_by(),
            player_count,
            max_players: PUBLIC_LOBBY_PLAYER_CAPACITY,
            open_slots: PUBLIC_LOBBY_PLAYER_CAPACITY.saturating_sub(player_count),
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            selected_game: self
                .selected_game
                .as_ref()
                .map(PublicLobbyGameSummary::from_selection),
        })
    }

    fn public_player_count(&self) -> u8 {
        self.players
            .iter()
            .filter(|player| player.occupied)
            .take(usize::from(PUBLIC_LOBBY_PLAYER_CAPACITY))
            .count()
            .try_into()
            .unwrap_or(PUBLIC_LOBBY_PLAYER_CAPACITY)
    }

    fn host_is_connected(&self) -> bool {
        self.players.iter().any(|player| {
            player.player_index == 0
                && player.connected
                && player.status == LobbyPlayerStatus::Connected
        })
    }

    fn hosted_by(&self) -> String {
        self.players
            .iter()
            .find(|player| player.player_index == 0)
            .and_then(|player| player.display_name.as_deref())
            .filter(|display_name| !display_name.trim().is_empty())
            .unwrap_or("Player 1")
            .to_string()
    }
}

impl PublicLobbyGameSummary {
    fn from_selection(selection: &LobbyGameSelectionView) -> Self {
        Self {
            title: selection.game.title.clone(),
            system_id: selection.game.system_id.clone(),
            core_id: selection.game.core_id.clone(),
            start_state_label: selection.game.start_state_label.clone(),
        }
    }
}
