//! Domain events emitted by room state changes.
//!
//! Events let transports broadcast room changes without putting WebSocket
//! concepts inside the room domain model.

use crate::protocol::{
    InputFrame, LinkCablePacket, SessionPauseView, SnapshotChunk, SnapshotManifest,
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
}
