//! Room-specific errors.
//!
//! These errors describe domain failures before they are mapped to HTTP or
//! WebSocket protocol responses.

use crate::rooms::PlayerIndex;

/// Failure while mutating or querying room state.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum RoomError {
    /// Invite code did not match an active room.
    #[error("room was not found")]
    NotFound,
    /// The room has no empty player slots.
    #[error("room is full")]
    RoomFull,
    /// The room has been closed.
    #[error("room is closed")]
    RoomClosed,
    /// The provided invite code was malformed.
    #[error("invite code is invalid")]
    InvalidInviteCode,
    /// The connection is not attached to a player slot in this room.
    #[error("connection is not assigned to a player slot")]
    UnknownConnection,
    /// A client attempted to send input for a slot it does not own.
    #[error("connection cannot send input for player {0:?}")]
    SlotSpoofing(PlayerIndex),
    /// A socket attempted to attach as host without owning the host subject.
    #[error("connection cannot attach as this room host")]
    HostSubjectMismatch,
    /// Input frame was older than the most recently accepted frame.
    #[error("input frame is out of order")]
    OutOfOrderFrame,
    /// Input frame was too far ahead of the room frame.
    #[error("input frame is too far ahead")]
    FutureFrameTooLarge,
    /// Input arrived while the room was not in a playable state.
    #[error("room is not playing")]
    NotPlaying,
    /// The room has not reached the required sync/ready phase.
    #[error("room is not ready for this operation")]
    RoomNotReady,
    /// A guest attempted a host-only operation.
    #[error("only the host can perform this operation")]
    HostOnly,
    /// Snapshot payload failed relay validation.
    #[error("snapshot payload is invalid")]
    SnapshotInvalid,
    /// Large snapshot file relay is unavailable for this room or server.
    #[error("snapshot file relay is unavailable")]
    SnapshotFileRelayUnavailable,
    /// Link-cable packet failed relay validation.
    ///
    /// The class is a static, sanitized operator diagnostic. It intentionally
    /// carries no packet bytes, client-controlled text, credentials, or room
    /// identifiers.
    #[error("link-cable packet is invalid ({diagnostic_class})")]
    LinkPacketInvalid {
        /// Stable private-provider rejection class.
        diagnostic_class: &'static str,
    },
    /// Link-cable packet sequence did not increase.
    #[error("link-cable packet is out of order")]
    OutOfOrderLinkPacket,
    /// Generic client payload failed validation.
    #[error("client payload is invalid")]
    InvalidPayload,
    /// Connected players do not have matching compatibility fingerprints.
    #[error("netplay compatibility mismatch")]
    CompatibilityMismatch,
    /// Client tried to use a room epoch that is no longer current.
    #[error("room epoch is stale")]
    StaleRoomEpoch,
    /// Client tried to use a session epoch that is no longer current.
    #[error("session epoch is stale")]
    StaleSessionEpoch,
    /// Reconnect token was missing, expired, or did not match the player slot.
    #[error("resume token is invalid")]
    ResumeTokenInvalid,
    /// Reconnect grace elapsed before the player reclaimed the slot.
    #[error("recovery window expired")]
    RecoveryExpired,
    /// Voice was not configured or a token refresh could not be completed.
    #[error("voice chat is unavailable")]
    VoiceUnavailable,
}
