//! Server-to-client WebSocket messages.
//!
//! These messages are stable room updates or protocol errors Desktop can render
//! directly. They do not contain secrets or raw auth details.

use crate::protocol::{
    InputFrame, LinkCablePacket, SessionPauseView, SnapshotChunk, SnapshotManifest,
};
use crate::rooms::RoomView;
use serde::Serialize;

/// Message sent by the relay over a room WebSocket.
#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerMessage {
    /// Initial message after a socket joins a room.
    RoomJoined {
        /// Zero-based player index assigned to this connection.
        your_player_index: u8,
        /// Current room state.
        room: RoomView,
    },
    /// Room state changed after join, disconnect, compatibility, or start.
    RoomStateChanged {
        /// Current room state.
        room: RoomView,
    },
    /// Relay keepalive response.
    Pong,
    /// Session can begin from the supplied canonical start frame.
    StartSession {
        /// Canonical frame both clients should start from.
        start_frame: u64,
        /// Current room state.
        room: RoomView,
    },
    /// Input accepted and relayed from a player.
    InputFrame {
        /// Authoritative input frame.
        input: InputFrame,
    },
    /// Link-cable packet relayed from another player.
    LinkCablePacket {
        /// Opaque virtual cable packet.
        packet: LinkCablePacket,
    },
    /// Snapshot chunk relayed from the host.
    SnapshotChunk {
        /// Snapshot chunk payload.
        chunk: SnapshotChunk,
    },
    /// Snapshot transfer completion manifest relayed from host.
    SnapshotComplete {
        /// Snapshot manifest.
        manifest: SnapshotManifest,
    },
    /// Session pause was scheduled for a future canonical frame.
    SessionPauseScheduled {
        /// Current pause state.
        pause: SessionPauseView,
        /// Current room state.
        room: RoomView,
    },
    /// Session pause holder or acknowledgement state changed.
    SessionPauseUpdated {
        /// Current pause state.
        pause: SessionPauseView,
        /// Current room state.
        room: RoomView,
    },
    /// Session can resume after every pause holder was released.
    SessionResumeScheduled {
        /// Pause sequence being resumed.
        sequence: u64,
        /// Frame where clients resume from.
        resume_at_frame: u64,
        /// Current room state.
        room: RoomView,
    },
    /// Stable protocol error safe to show in Desktop.
    Error {
        /// Stable error code.
        code: String,
        /// Human-readable message.
        message: String,
    },
}
