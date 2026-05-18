//! Coordinated pause protocol values.
//!
//! These DTOs are shared by room views and WebSocket messages. The room domain
//! owns mutation rules; this module only defines the wire-safe shape.

use crate::rooms::PlayerIndex;
use serde::{Deserialize, Serialize};

/// Reason a client requests a coordinated netplay pause.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionPauseReason {
    /// Player opened the in-game menu.
    Menu,
    /// Platform/app lifecycle backgrounded the client.
    Backgrounded,
    /// Runtime or system-level pause.
    System,
    /// Relay paused because a player connection dropped.
    ConnectionLost,
}

/// Current state of a coordinated pause.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionPauseState {
    /// Pause has been scheduled but every client has not acknowledged yet.
    Pausing,
    /// Every connected client acknowledged the pause frame.
    Paused,
}

/// A player currently holding the room paused.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPauseHolder {
    /// Player index holding the pause.
    pub player_index: PlayerIndex,
    /// Reason this player is holding the pause.
    pub reason: SessionPauseReason,
}

/// Serializable coordinated pause state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPauseView {
    /// Monotonic sequence for this pause lifecycle.
    pub sequence: u64,
    /// Pause state.
    pub state: SessionPauseState,
    /// Original pause reason.
    pub reason: SessionPauseReason,
    /// Player that created this pause lifecycle.
    pub requested_by_player_index: PlayerIndex,
    /// Canonical frame where every client should stop.
    pub pause_at_frame: u64,
    /// Canonical frame where every client actually stopped after ack.
    pub paused_at_frame: Option<u64>,
    /// Players that acknowledged the pause frame.
    pub acknowledged_player_indexes: Vec<PlayerIndex>,
    /// Players currently holding the room paused.
    pub holders: Vec<SessionPauseHolder>,
}
