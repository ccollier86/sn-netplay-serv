//! Server-to-client WebSocket messages.
//!
//! These messages are stable room updates or protocol errors Desktop can render
//! directly. They do not contain secrets or raw auth details.

use crate::protocol::{
    ClockSyncPong, ClockSyncSampleRequest, InputDelayChange, InputFrame, LinkCableDataPlaneGrant,
    LinkCablePacket, RomRelayBlocked as RomRelayBlockedPayload,
    RomRelayCancelled as RomRelayCancelledPayload, RomRelayCompletion as RomRelayCompletionPayload,
    RomRelayFailure as RomRelayFailurePayload, RomRelayGrant,
    RomRelayProgress as RomRelayProgressPayload, ScheduledSessionStart, SessionPauseView,
    SnapshotChunk, SnapshotFileRelayGrant, SnapshotManifest, StateHashMismatchView,
    StateRecoveryView,
};
use crate::rooms::{PlayerVoiceJoinGrant, RoomView};
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
        ///
        /// Absent for link-cable rooms, which use a dedicated private route.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_socket_token: Option<String>,
        /// Optional player-specific voice grant.
        voice: Option<PlayerVoiceJoinGrant>,
        /// Private link route metadata, absent from every controller room.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        link_cable_grant: Option<LinkCableDataPlaneGrant>,
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
    /// Server request for a short startup clock-sample burst.
    ClockSyncSampleRequested {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Clock-sample request payload.
        request: ClockSyncSampleRequest,
    },
    /// Server response to a client-originated clock ping.
    ClockSyncPong {
        /// Clock ping response payload.
        pong: ClockSyncPong,
    },
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
        /// Optional future server-time start for v2 synchronized release.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scheduled_start: Option<ScheduledSessionStart>,
        /// Current room state.
        room: RoomView,
    },
    /// Input accepted and relayed from a player.
    InputFrame {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Authoritative input frame.
        input: InputFrame,
    },
    /// Link-cable packet relayed from another player.
    LinkCablePacket {
        /// Opaque virtual cable packet.
        packet: LinkCablePacket,
    },
    /// Private link route lifecycle changed for this authenticated endpoint.
    LinkCableGrantUpdated {
        /// Current private route and native-admission metadata.
        grant: LinkCableDataPlaneGrant,
    },
    /// Snapshot chunk relayed from the host.
    SnapshotChunk {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Snapshot chunk payload.
        chunk: SnapshotChunk,
    },
    /// Snapshot transfer completion manifest relayed from host.
    SnapshotComplete {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Snapshot manifest.
        manifest: SnapshotManifest,
    },
    /// Private host grant for uploading a large snapshot through the file relay.
    SnapshotFileRelayUploadGranted {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Upload grant for this client.
        grant: SnapshotFileRelayGrant,
    },
    /// Private guest grant for downloading a large snapshot from the file relay.
    SnapshotFileRelayDownloadReady {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Download grant for this client.
        grant: SnapshotFileRelayGrant,
    },
    /// Private host grant for uploading a temporary ROM through the file relay.
    #[serde(rename = "romRelay.grantUpload")]
    RomRelayGrantUpload {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Upload grant for this client.
        grant: RomRelayGrant,
    },
    /// Private guest grant for downloading a temporary ROM from the file relay.
    #[serde(rename = "romRelay.grantDownload")]
    RomRelayGrantDownload {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Download grant for this client.
        grant: RomRelayGrant,
    },
    /// Temporary ROM relay progress changed.
    #[serde(rename = "romRelay.progress")]
    RomRelayProgress {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Progress payload.
        progress: RomRelayProgressPayload,
    },
    /// Temporary ROM relay upload/download completed.
    #[serde(rename = "romRelay.completed")]
    RomRelayCompleted {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Completion payload.
        completion: RomRelayCompletionPayload,
    },
    /// Temporary ROM relay failed.
    #[serde(rename = "romRelay.failed")]
    RomRelayFailed {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Failure payload.
        failure: RomRelayFailurePayload,
    },
    /// Temporary ROM relay is blocked by policy/state.
    #[serde(rename = "romRelay.blocked")]
    RomRelayBlocked {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Block payload.
        blocked: RomRelayBlockedPayload,
    },
    /// Temporary ROM relay was cancelled.
    #[serde(rename = "romRelay.cancelled")]
    RomRelayCancelled {
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Cancel payload.
        cancelled: RomRelayCancelledPayload,
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
        /// New-epoch synchronized deadline for protocol v5.
        #[serde(skip_serializing_if = "Option::is_none")]
        scheduled_start: Option<ScheduledSessionStart>,
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
    /// Private response with a fresh voice token for this socket's player.
    VoiceTokenRefreshed {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Refreshed player-specific voice grant.
        voice: PlayerVoiceJoinGrant,
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
    /// Protocol v5 clients must freeze the old epoch while the host pins state.
    StateRecoveryPrepare {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Old room epoch that is being frozen.
        room_epoch: u64,
        /// Old session epoch that is being frozen.
        session_epoch: u64,
        /// Recovery transaction and exact repair frame.
        recovery: StateRecoveryView,
        /// Current room state.
        room: RoomView,
    },
    /// Protocol v5 recovery was atomically committed to a fresh session epoch.
    StateRecoveryCommitted {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Fresh session epoch clients must attach to.
        session_epoch: u64,
        /// Committed transaction and exact pinned snapshot manifest.
        recovery: StateRecoveryView,
        /// Current room state.
        room: RoomView,
    },
    /// Protocol v5 recovery failed and this gameplay room was closed.
    StateRecoveryFailed {
        /// Monotonic event sequence included with the room view.
        event_seq: u64,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
        /// Last known transaction state.
        recovery: StateRecoveryView,
        /// Stable machine-readable failure reason.
        reason: String,
        /// Closed room state.
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
