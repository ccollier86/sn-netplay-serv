//! Domain events emitted by room state changes.
//!
//! Events let transports broadcast room changes without putting WebSocket
//! concepts inside the room domain model.

use crate::protocol::{
    InputDelayChange, InputFrame, InputFrameBatch, LinkCablePacket, ServerFrame, SessionPauseView,
    SnapshotChunk, SnapshotManifest, StateHashMismatchView,
};
use crate::rooms::{ConnectionId, RoomView};

/// Event emitted after a room changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoomEvent {
    /// Serializable room state should be broadcast to subscribers.
    RoomStateChanged(RoomView),
    /// Room reached the gameplay start state.
    SessionStarted { start_frame: u64, room: RoomView },
    /// Room scheduled a coordinated pause.
    SessionPauseScheduled {
        /// Current pause state.
        pause: SessionPauseView,
        /// Current room state.
        room: RoomView,
    },
    /// Room pause holder or acknowledgement state changed.
    SessionPauseUpdated {
        /// Current pause state.
        pause: SessionPauseView,
        /// Current room state.
        room: RoomView,
    },
    /// Room can resume after every pause holder was released.
    SessionResumeScheduled {
        /// Pause sequence being resumed.
        sequence: u64,
        /// Frame clients resume from.
        resume_at_frame: u64,
        /// Current room state.
        room: RoomView,
    },
    /// Validated input frame should be relayed to subscribers.
    InputFrame {
        /// Connection that supplied the input frame.
        source: ConnectionId,
        /// Validated input frame.
        input: InputFrame,
    },
    /// Link-cable packet should be relayed to other subscribers.
    LinkCablePacket {
        /// Connection that supplied the packet.
        source: ConnectionId,
        /// Validated virtual link-cable packet.
        packet: LinkCablePacket,
    },
    /// Snapshot chunk should be relayed to subscribers.
    SnapshotChunk {
        /// Connection that supplied the snapshot chunk.
        source: ConnectionId,
        /// Validated snapshot chunk.
        chunk: SnapshotChunk,
    },
    /// Snapshot manifest should be relayed to subscribers.
    SnapshotComplete {
        /// Connection that supplied the snapshot manifest.
        source: ConnectionId,
        /// Validated snapshot manifest.
        manifest: SnapshotManifest,
    },
    /// One player intentionally left the room.
    PlayerExited {
        /// Zero-based player index that quit.
        player_index: u8,
        /// Short reason safe for peer UI and diagnostics.
        reason: String,
        /// Current room state after the exit.
        room: RoomView,
    },
    /// Deterministic state hash mismatch was detected.
    StateHashMismatch {
        /// Mismatch details.
        mismatch: StateHashMismatchView,
        /// Current room state.
        room: RoomView,
    },
    /// Relay scheduled an adaptive input-delay change.
    InputDelayChanged {
        /// Scheduled delay update.
        change: InputDelayChange,
        /// Current room state.
        room: RoomView,
    },
}

/// Event emitted on the dedicated binary input relay channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoomInputEvent {
    /// Validated input frames should be relayed over binary input sockets.
    InputFrameBatch {
        /// Connection that supplied the input frames.
        source: ConnectionId,
        /// Validated input frame batch.
        batch: InputFrameBatch,
    },
    /// Canonical server frame released to every input socket.
    ServerFrame {
        /// Relay-owned frame release cursor.
        frame: ServerFrame,
    },
}
