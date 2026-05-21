//! Server-to-client WebSocket messages.
//!
//! These messages are stable room updates or protocol errors Desktop can render
//! directly. They do not contain secrets or raw auth details.

use crate::protocol::{
    InputDelayChange, InputFrame, LinkCablePacket, SessionPauseView, SnapshotChunk,
    SnapshotManifest, StateHashMismatchView,
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
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Zero-based player index assigned to this connection.
        your_player_index: u8,
        /// Opaque token this player can use to reclaim the same slot.
        resume_token: String,
        /// Opaque token this player uses to attach the binary input socket.
        input_socket_token: String,
        /// Current room state.
        room: RoomView,
    },
    /// Room state changed after join, disconnect, compatibility, or start.
    RoomStateChanged {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Current room state.
        room: RoomView,
    },
    /// Relay keepalive response.
    Pong,
    /// Session can begin from the supplied canonical start frame.
    StartSession {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
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
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Current pause state.
        pause: SessionPauseView,
        /// Current room state.
        room: RoomView,
    },
    /// Session pause holder or acknowledgement state changed.
    SessionPauseUpdated {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Current pause state.
        pause: SessionPauseView,
        /// Current room state.
        room: RoomView,
    },
    /// Session can resume after every pause holder was released.
    SessionResumeScheduled {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Pause sequence being resumed.
        sequence: u64,
        /// Frame where clients resume from.
        resume_at_frame: u64,
        /// Current room state.
        room: RoomView,
    },
    /// Relay requests clients to resend compatibility.
    CompatibilityRequested {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Current room state.
        room: RoomView,
    },
    /// Relay entered recovery because a player disconnected.
    RecoveryStarted {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Current room state.
        room: RoomView,
    },
    /// Relay accepted a reconnect for one player slot.
    PlayerReconnected {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Reclaimed zero-based player index.
        player_index: u8,
        /// Current room state.
        room: RoomView,
    },
    /// A player intentionally quit the room.
    PlayerExited {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Zero-based player index that quit.
        player_index: u8,
        /// Short client-provided reason for diagnostics and peer UI.
        reason: String,
        /// Current room state.
        room: RoomView,
    },
    /// Relay requires a compatibility check and state sync after recovery.
    RecoveryResyncRequired {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Current room state.
        room: RoomView,
    },
    /// Relay detected mismatched deterministic state hashes.
    StateHashMismatch {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Mismatch details.
        mismatch: StateHashMismatchView,
        /// Current room state.
        room: RoomView,
    },
    /// Relay scheduled an adaptive controller input-delay change.
    InputDelayChanged {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Scheduled delay update.
        change: InputDelayChange,
        /// Current room state.
        room: RoomView,
    },
    /// Response to app-level heartbeat.
    HeartbeatAck {
        /// Latest server event sequence.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
    },
    /// Stable protocol error safe to show in Desktop.
    Error {
        /// Stable error code.
        code: String,
        /// Human-readable message.
        message: String,
    },
}
