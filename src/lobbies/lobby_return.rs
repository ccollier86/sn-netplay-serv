//! Lobby return-to-setup metadata.
//!
//! This module owns the serializable reason and attribution DTOs for returning
//! from an active game room to a lobby. It must not mutate lobby state, open
//! sockets, or decide whether a return request is valid.

use crate::rooms::{ConnectionId, PlayerIndex};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Reason a lobby gameplay session returned from the active game.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyReturnReason {
    /// A player explicitly chose to return to the lobby.
    PlayerRequestedReturn,
    /// The runner window or process closed.
    RunnerClosed,
    /// A remote runner disconnected from active gameplay.
    RemoteDisconnected,
    /// The gameplay room closed.
    RoomClosed,
    /// Runner bootstrap failed before gameplay began.
    LaunchFailed,
    /// An unrecoverable netplay error ended gameplay.
    NetplayError,
    /// An emulator/backend error ended gameplay.
    EmulatorError,
    /// The app synthesized recovery after the runner exited without a report.
    RunnerCrashed,
}

/// Client request metadata for returning an active game to lobby setup.
pub struct LobbyReturnRequest {
    /// Lobby socket reporting the return.
    pub connection_id: ConnectionId,
    /// Lobby epoch observed by the reporting client.
    pub lobby_epoch: u64,
    /// Selected game proposal that was active.
    pub proposal_id: Uuid,
    /// Player index that caused the return, if known.
    pub return_requested_by_player_index: Option<PlayerIndex>,
    /// Runner/app reason for returning to the lobby, if known.
    pub reason: Option<LobbyReturnReason>,
    /// Server receive timestamp in milliseconds since unix epoch.
    pub now_ms: u128,
}

/// Server-broadcast return-to-lobby attribution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyReturnedView {
    /// Selected game proposal that returned to lobby setup.
    pub proposal_id: Uuid,
    /// Player index that caused the return, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_requested_by_player_index: Option<u8>,
    /// Return reason supplied by the reporting client, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<LobbyReturnReason>,
    /// Milliseconds since Unix epoch when the server accepted the return.
    pub returned_at_ms: u128,
}

impl LobbyReturnedView {
    /// Creates a sanitized return attribution view for lobby broadcasts.
    pub fn new(
        proposal_id: Uuid,
        return_requested_by_player_index: Option<PlayerIndex>,
        reason: Option<LobbyReturnReason>,
        returned_at_ms: u128,
    ) -> Self {
        Self {
            proposal_id,
            return_requested_by_player_index: return_requested_by_player_index
                .map(PlayerIndex::zero_based),
            reason,
            returned_at_ms,
        }
    }
}

/// Result of reducing one return-to-lobby request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyReturnOutcome {
    /// Metadata to broadcast or reuse for idempotent duplicate returns.
    pub returned: LobbyReturnedView,
    /// Whether this request changed authoritative lobby state.
    pub state_changed: bool,
}

impl LobbyReturnOutcome {
    /// Builds an outcome for the first accepted return from an active launch.
    pub fn applied(returned: LobbyReturnedView) -> Self {
        Self {
            returned,
            state_changed: true,
        }
    }

    /// Builds an outcome for a duplicate return that was already reduced.
    pub fn already_applied(returned: LobbyReturnedView) -> Self {
        Self {
            returned,
            state_changed: false,
        }
    }
}
