//! Relay decision for validated controller input frames.

/// Result of accepting one input frame into room state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputFrameAcceptance {
    /// Relay the frame to the other connected players.
    Relay,
    /// Drop the frame without failing the socket.
    Ignore,
}
