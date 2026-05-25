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
}

impl LobbyGameLaunchView {
    /// Creates a launch view for the selected game proposal.
    pub fn new(proposal_id: Uuid, requested_by: PlayerIndex, requested_at_ms: u128) -> Self {
        Self {
            proposal_id,
            requested_by_player_index: requested_by.zero_based(),
            requested_at_ms,
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
