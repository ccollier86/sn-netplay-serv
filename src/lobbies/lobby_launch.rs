//! Lobby game readiness and launch DTOs.
//!
//! Readiness is scoped to a selected game proposal so stale client state cannot
//! accidentally launch a different ROM after the host changes games.

use crate::lobbies::LobbyError;
use crate::rooms::PlayerIndex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client-reported readiness for the currently selected lobby game.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyGameReadinessStatus {
    /// Client has not evaluated the selected game yet.
    Unknown,
    /// Client can launch this selected game.
    Ready,
    /// Client is present but not ready yet.
    NotReady,
    /// Client does not have a matching ROM yet.
    MissingRom,
    /// Client does not have the selected startup save-state material yet.
    MissingStartupState,
    /// Client cannot run the selected game/core.
    Unsupported,
}

/// Per-player readiness view returned with the lobby state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyGameReadinessView {
    /// Player slot reporting readiness.
    pub player_index: u8,
    /// Selected game proposal this status belongs to.
    pub proposal_id: Uuid,
    /// Current readiness status.
    pub status: LobbyGameReadinessStatus,
    /// Optional short detail for UI and diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Milliseconds since unix epoch when readiness changed.
    pub updated_at_ms: u128,
}

impl LobbyGameReadinessView {
    /// Creates a sanitized readiness entry for one player.
    pub fn new(
        player_index: PlayerIndex,
        proposal_id: Uuid,
        status: LobbyGameReadinessStatus,
        detail: Option<String>,
        updated_at_ms: u128,
    ) -> Result<Self, LobbyError> {
        Ok(Self {
            player_index: player_index.zero_based(),
            proposal_id,
            status,
            detail: sanitize_readiness_detail(detail)?,
            updated_at_ms,
        })
    }
}

/// Host launch signal returned with lobby state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyGameLaunchView {
    /// Selected game proposal being launched.
    pub proposal_id: Uuid,
    /// Host player that requested launch.
    pub requested_by_player_index: u8,
    /// Milliseconds since unix epoch when launch was requested.
    pub requested_at_ms: u128,
    /// Current handoff status.
    pub status: LobbyGameLaunchStatus,
    /// Gameplay room invite code once the host has created it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_invite_code: Option<String>,
    /// Milliseconds since unix epoch when the gameplay room was published.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_published_at_ms: Option<u128>,
    /// Milliseconds since unix epoch when gameplay was confirmed running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gameplay_started_at_ms: Option<u128>,
    /// Player indexes whose runners have reported deterministic gameplay start.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub started_player_indexes: Vec<u8>,
}

/// Handoff state for launching the selected lobby game.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyGameLaunchStatus {
    /// Host has asked all clients to prepare for launch.
    Preparing,
    /// Host published the direct gameplay room invite code.
    Ready,
    /// A launched runner reported that deterministic gameplay is active.
    Playing,
}

impl LobbyGameLaunchView {
    /// Creates a launch view for the selected game proposal.
    pub fn new(proposal_id: Uuid, requested_by: PlayerIndex, requested_at_ms: u128) -> Self {
        Self {
            proposal_id,
            requested_by_player_index: requested_by.zero_based(),
            requested_at_ms,
            status: LobbyGameLaunchStatus::Preparing,
            room_invite_code: None,
            room_published_at_ms: None,
            gameplay_started_at_ms: None,
            started_player_indexes: Vec::new(),
        }
    }

    /// Records the gameplay room invite once host setup completes.
    pub fn publish_room(&mut self, invite_code: String, published_at_ms: u128) {
        self.status = LobbyGameLaunchStatus::Ready;
        self.room_invite_code = Some(invite_code);
        self.room_published_at_ms = Some(published_at_ms);
    }

    /// Records one player's runner start report and marks the launch playing
    /// once every expected player has reported.
    pub fn mark_player_started(
        &mut self,
        player_index: PlayerIndex,
        expected_player_indexes: &[PlayerIndex],
        started_at_ms: u128,
    ) -> Result<bool, LobbyError> {
        match self.status {
            LobbyGameLaunchStatus::Preparing => Err(LobbyError::GameLaunchNotReady),
            LobbyGameLaunchStatus::Ready => {
                let player_index = player_index.zero_based();
                if self.started_player_indexes.contains(&player_index) {
                    return Ok(false);
                }
                self.started_player_indexes.push(player_index);
                self.started_player_indexes.sort_unstable();
                if !expected_player_indexes.is_empty()
                    && expected_player_indexes.iter().all(|expected| {
                        self.started_player_indexes.contains(&expected.zero_based())
                    })
                {
                    self.status = LobbyGameLaunchStatus::Playing;
                    self.gameplay_started_at_ms = Some(started_at_ms);
                }
                Ok(true)
            }
            LobbyGameLaunchStatus::Playing => Ok(false),
        }
    }
}

fn sanitize_readiness_detail(detail: Option<String>) -> Result<Option<String>, LobbyError> {
    let Some(detail) = detail else {
        return Ok(None);
    };
    let sanitized = detail
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let sanitized = sanitized.trim();

    if sanitized.is_empty() {
        return Ok(None);
    }
    if sanitized.chars().count() > 160 {
        return Err(LobbyError::InvalidPayload);
    }

    Ok(Some(sanitized.to_string()))
}
