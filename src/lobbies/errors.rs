//! Lobby domain errors.
//!
//! These errors describe lobby state failures only. HTTP status mapping stays in
//! the HTTP layer so the domain remains transport-neutral.

use thiserror::Error;

/// Error returned by lobby registry operations.
#[derive(Debug, Error)]
pub enum LobbyError {
    /// Invite code did not match an active lobby.
    #[error("lobby not found")]
    NotFound,
    /// Lobby cannot accept more players.
    #[error("lobby is full")]
    LobbyFull,
    /// Lobby has been closed.
    #[error("lobby is closed")]
    LobbyClosed,
    /// Client tried to mutate stale lobby state.
    #[error("stale lobby epoch")]
    StaleLobbyEpoch,
    /// Reconnect token did not match the requested player slot.
    #[error("resume token is invalid")]
    ResumeTokenInvalid,
    /// Player slot could not be used by the requesting subject.
    #[error("player slot is not available")]
    PlayerSlotUnavailable,
}
