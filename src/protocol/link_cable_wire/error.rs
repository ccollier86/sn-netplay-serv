//! SBLK v1 codec failures.

/// Byte-shape or event-local validation failure for an SBLK v1 frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LinkCableWireCodecError {
    /// The resolved protocol namespace was not one of the frozen v1 namespaces.
    #[error("link cable wire protocol is unsupported")]
    UnsupportedProtocol,
    /// The frame fell outside the frozen 43..=128 byte size range.
    #[error("link cable wire frame size is invalid")]
    InvalidFrameSize,
    /// The frame did not start with the literal SBLK magic.
    #[error("link cable wire magic is unsupported")]
    UnsupportedMagic,
    /// The frame did not use SBLK wire version 1.
    #[error("link cable wire version is unsupported")]
    UnsupportedVersion,
    /// A reserved SBLK header flag was set.
    #[error("link cable wire reserved flags must be zero")]
    ReservedFlagsSet,
    /// A u64 header or body field used the high bit reserved by v1.
    #[error("link cable wire integer exceeds the signed 64-bit range")]
    HighBitSet,
    /// A sender or clock-owner slot was not 0 or 1.
    #[error("link cable wire slot must be 0 or 1")]
    InvalidSlot,
    /// The declared body length did not consume the complete frame.
    #[error("link cable wire body length does not match the frame")]
    BodyLengthMismatch,
    /// The selected namespace does not define the supplied event kind.
    #[error("link cable wire event kind is unsupported for the resolved protocol")]
    UnsupportedEventKind,
    /// The body length was not the exact frozen length for its event kind.
    #[error("link cable wire event body length is invalid")]
    InvalidEventBodyLength,
    /// A transfer identifier was zero.
    #[error("link cable transfer identifier must be nonzero")]
    InvalidTransferId,
    /// An event was emitted by a slot that cannot own that event role.
    #[error("link cable event sender does not own the event role")]
    InvalidEventRole,
    /// A GBA mode value or its register snapshot was invalid.
    #[error("GBA link cable mode snapshot is invalid")]
    InvalidGbaMode,
    /// A GBA transfer-start register snapshot was not the required pre-start state.
    #[error("GBA link cable transfer-start snapshot is invalid")]
    InvalidGbaTransferStart,
    /// A two-player GBA commit did not mark slots 2 and 3 disconnected.
    #[error("GBA link cable commit has invalid disconnected-slot words")]
    InvalidGbaDisconnectedWords,
    /// A GB serial-start control byte was not exactly 0x81 or 0x83.
    #[error("GB link cable serial control is unsupported")]
    InvalidGbSerialControl,
    /// An abort reason was outside the frozen 1..=5 range.
    #[error("link cable abort reason is unsupported")]
    InvalidAbortReason,
}
