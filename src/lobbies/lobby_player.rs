//! Lobby player slots.
//!
//! Lobby players are independent from active game-room sockets. A player can
//! leave a game and return to the same lobby slot later.

use crate::auth::{ClientKind, VerifiedLicense};
use crate::lobbies::LobbyClientCapabilities;
use crate::rooms::{ConnectionId, PlayerIndex, ResumeTokenHash};
use serde::Serialize;

const PLAYER_COLORS: [&str; 4] = ["cyan", "violet", "amber", "emerald"];

/// Server-assigned lobby role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyPlayerRole {
    /// Lobby creator and Player 1.
    Host,
    /// Joined player.
    Guest,
}

/// User-facing lobby player status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyPlayerStatus {
    /// Slot is empty.
    Empty,
    /// Player is present in the lobby.
    Connected,
    /// Player can reclaim this slot with a resume token.
    Reconnecting,
    /// Player intentionally left or recovery expired.
    Disconnected,
}

/// Mutable lobby slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyPlayerSlot {
    /// Zero-based player index.
    pub player_index: PlayerIndex,
    /// Server-assigned role.
    pub role: LobbyPlayerRole,
    /// User-facing accent color.
    pub color: &'static str,
    /// Stable authenticated subject key occupying this slot.
    pub subject_key: Option<String>,
    /// Client platform for this slot.
    pub client_kind: Option<ClientKind>,
    /// Active lobby control connection.
    pub connection_id: Option<ConnectionId>,
    /// Optional display name chosen by the player.
    pub display_name: Option<String>,
    /// Current slot status.
    pub status: LobbyPlayerStatus,
    /// Client-reported lobby capabilities.
    pub capabilities: LobbyClientCapabilities,
    /// One-way hash of the slot resume token.
    pub resume_token_hash: Option<ResumeTokenHash>,
    /// Last activity timestamp in milliseconds since unix epoch.
    pub last_seen_at_ms: Option<u128>,
}

/// Serializable lobby slot view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyPlayerSlotView {
    /// Zero-based player index.
    pub player_index: u8,
    /// One-based player number shown in UI.
    pub display_number: u8,
    /// Server-assigned role.
    pub role: LobbyPlayerRole,
    /// User-facing accent color.
    pub color: String,
    /// Current slot status.
    pub status: LobbyPlayerStatus,
    /// Whether a verified player occupies this slot.
    pub occupied: bool,
    /// Whether the lobby control connection is active.
    pub connected: bool,
    /// Client platform occupying this slot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<ClientKind>,
    /// Optional player display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Client-reported feature support.
    pub capabilities: LobbyClientCapabilities,
    /// Last activity timestamp in milliseconds since unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at_ms: Option<u128>,
}

/// Verified player claim used to occupy or refresh a lobby slot.
pub struct LobbyPlayerOccupancy<'a> {
    /// Server-assigned role for the occupied slot.
    pub role: LobbyPlayerRole,
    /// Verified license for the player occupying this slot.
    pub license: &'a VerifiedLicense,
    /// Active lobby control connection.
    pub connection_id: ConnectionId,
    /// Optional display name chosen by the player.
    pub display_name: Option<String>,
    /// Client-reported lobby capabilities.
    pub capabilities: LobbyClientCapabilities,
    /// One-way hash of the issued resume token.
    pub resume_token_hash: ResumeTokenHash,
    /// Activity timestamp in milliseconds since unix epoch.
    pub now_ms: u128,
}

impl LobbyPlayerSlot {
    /// Creates an empty lobby player slot.
    pub fn empty(player_index: PlayerIndex) -> Self {
        Self {
            player_index,
            role: LobbyPlayerRole::Guest,
            color: player_color(player_index),
            subject_key: None,
            client_kind: None,
            connection_id: None,
            display_name: None,
            status: LobbyPlayerStatus::Empty,
            capabilities: LobbyClientCapabilities::default(),
            resume_token_hash: None,
            last_seen_at_ms: None,
        }
    }

    /// Creates the occupied host slot.
    pub fn host(
        license: &VerifiedLicense,
        connection_id: ConnectionId,
        display_name: Option<String>,
        capabilities: LobbyClientCapabilities,
        resume_token_hash: ResumeTokenHash,
        now_ms: u128,
    ) -> Self {
        let mut slot = Self::empty(PlayerIndex::ONE);
        slot.occupy(LobbyPlayerOccupancy {
            role: LobbyPlayerRole::Host,
            license,
            connection_id,
            display_name,
            capabilities,
            resume_token_hash,
            now_ms,
        });
        slot
    }

    /// Returns whether this slot is currently empty.
    pub fn is_empty(&self) -> bool {
        self.status == LobbyPlayerStatus::Empty
    }

    /// Returns whether this slot belongs to the verified subject.
    pub fn belongs_to(&self, license: &VerifiedLicense) -> bool {
        self.subject_key
            .as_ref()
            .is_some_and(|value| value == &license.identity_key())
    }

    /// Occupies or refreshes a slot with verified player data.
    pub fn occupy(&mut self, occupancy: LobbyPlayerOccupancy<'_>) {
        self.role = occupancy.role;
        self.subject_key = Some(occupancy.license.identity_key());
        self.client_kind = Some(occupancy.license.client_kind);
        self.connection_id = Some(occupancy.connection_id);
        if occupancy.display_name.is_some() {
            self.display_name = occupancy.display_name;
        }
        self.status = LobbyPlayerStatus::Connected;
        self.capabilities = occupancy.capabilities;
        self.resume_token_hash = Some(occupancy.resume_token_hash);
        self.last_seen_at_ms = Some(occupancy.now_ms);
    }

    /// Converts the slot into the API view.
    pub fn view(&self) -> LobbyPlayerSlotView {
        LobbyPlayerSlotView {
            player_index: self.player_index.zero_based(),
            display_number: self.player_index.display_number(),
            role: self.role,
            color: self.color.to_string(),
            status: self.status,
            occupied: self.subject_key.is_some(),
            connected: self.connection_id.is_some(),
            client_kind: self.client_kind,
            display_name: self.display_name.clone(),
            capabilities: self.capabilities.clone(),
            last_seen_at_ms: self.last_seen_at_ms,
        }
    }
}

fn player_color(player_index: PlayerIndex) -> &'static str {
    PLAYER_COLORS
        .get(usize::from(player_index.zero_based()))
        .copied()
        .unwrap_or("cyan")
}
