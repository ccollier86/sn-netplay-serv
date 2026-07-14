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
    /// A non-host connection attempted to remove a lobby player.
    #[error("only the lobby host can remove a player")]
    PlayerRemovalHostOnly,
    /// The requested lobby player slot is empty.
    #[error("lobby player was not found")]
    LobbyPlayerNotFound,
    /// The host slot cannot be removed.
    #[error("the lobby host cannot be removed")]
    CannotRemoveLobbyHost,
    /// Current lobby state does not permit player removal.
    #[error("lobby player removal is unavailable")]
    LobbyPlayerRemovalUnavailable,
    /// Selected game changed or no longer exists.
    #[error("lobby game proposal is stale")]
    StaleGameProposal,
    /// One or more connected players are not ready to launch.
    #[error("lobby players are not ready")]
    PlayersNotReady,
    /// Gameplay cannot be marked active before the room handoff is ready.
    #[error("lobby game launch is not ready")]
    GameLaunchNotReady,
    /// Temporary ROM relay is disabled or unavailable.
    #[error("temporary rom relay is unavailable")]
    RomRelayUnavailable,
    /// One of the lobby clients cannot use temporary ROM relay.
    #[error("temporary rom relay is unsupported by a client")]
    RomRelayUnsupported,
    /// Proposed ROM exceeds the relay limit.
    #[error("temporary rom relay payload is too large")]
    RomRelayTooLarge,
    /// Selected startup-state relay is disabled or unavailable.
    #[error("startup state relay is unavailable")]
    StartupStateRelayUnavailable,
    /// One of the lobby clients cannot use selected startup-state relay.
    #[error("startup state relay is unsupported by a client")]
    StartupStateRelayUnsupported,
    /// Proposed startup state exceeds the relay limit.
    #[error("startup state relay payload is too large")]
    StartupStateRelayTooLarge,
    /// Lobby voice is disabled or unavailable.
    #[error("lobby voice is unavailable")]
    VoiceUnavailable,
    /// Client supplied malformed lobby data.
    #[error("invalid lobby payload")]
    InvalidPayload,
}
