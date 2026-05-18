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
    /// Link-cable packet failed relay validation.
    #[error("link-cable packet is invalid")]
    LinkPacketInvalid,
    /// Link-cable packet sequence did not increase.
    #[error("link-cable packet is out of order")]
    OutOfOrderLinkPacket,
    /// Connected players do not have matching compatibility fingerprints.
    #[error("netplay compatibility mismatch")]
    CompatibilityMismatch,
}
