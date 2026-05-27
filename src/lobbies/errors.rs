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
    /// Socket does not belong to this lobby.
    #[error("unknown lobby connection")]
    UnknownConnection,
    /// Only Player 1 can perform this lobby operation.
    #[error("host only lobby operation")]
    HostOnly,
    /// Selected game changed or no longer exists.
    #[error("lobby game proposal is stale")]
    StaleGameProposal,
    /// One or more connected players are not ready to launch.
    #[error("lobby players are not ready")]
    PlayersNotReady,
    /// Temporary ROM relay is disabled or unavailable.
    #[error("temporary rom relay is unavailable")]
    RomRelayUnavailable,
    /// One of the lobby clients cannot use temporary ROM relay.
    #[error("temporary rom relay is unsupported by a client")]
    RomRelayUnsupported,
    /// Proposed ROM exceeds the relay limit.
    #[error("temporary rom relay payload is too large")]
    RomRelayTooLarge,
    /// Lobby voice is disabled or unavailable.
    #[error("lobby voice is unavailable")]
    VoiceUnavailable,
    /// Client supplied malformed lobby data.
    #[error("invalid lobby payload")]
    InvalidPayload,
}
