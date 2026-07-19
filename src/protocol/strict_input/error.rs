//! Shared protocol v5 input-lane codec failures.

/// Protocol v5 input-lane codec failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StrictInputCodecError {
    /// Message size or field layout was invalid.
    #[error("strict input message is malformed")]
    Malformed,
    /// Magic or message discriminator was not recognized.
    #[error("strict input message type is unsupported")]
    Unsupported,
    /// Batch contained no inputs.
    #[error("strict input batch is empty")]
    Empty,
    /// Batch exceeded the protocol frame-count limit.
    #[error("strict input batch contains too many frames")]
    TooManyFrames,
    /// Message supplied a player outside the room capacity.
    #[error("strict input player index is invalid")]
    InvalidPlayerIndex,
    /// Input payload size did not match the negotiated v1 codec.
    #[error("strict input payload size is invalid")]
    InvalidPayloadSize,
    /// A NACK supplied an unknown reason code.
    #[error("strict input NACK reason is invalid")]
    InvalidNackReason,
    /// Release cursors were not uniquely sorted by player index.
    #[error("server frame release cursors are invalid")]
    InvalidCursors,
}
