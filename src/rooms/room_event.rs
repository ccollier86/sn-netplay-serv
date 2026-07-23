//! Domain events emitted by room state changes.
//!
//! Events let transports broadcast room changes without putting WebSocket
//! concepts inside the room domain model.

use crate::protocol::{
    FastInputFrame, InputDelayChange, InputFrame, InputFrameBatch, RomRelayCancelled,
    RomRelayCompletion, RomRelayFailure, RomRelayGrant, RomRelayProgress, ScheduledSessionStart,
    ServerFrame, ServerFrameReleaseV5, SessionPauseView, SnapshotChunk, SnapshotFileRelayGrant,
    SnapshotManifest, StateHashMismatchView, StateRecoveryView, StrictInputBatch,
};
use crate::rooms::{ConnectionId, RoomView};

/// Event emitted after a room changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoomEvent {
    /// Serializable room state should be broadcast to subscribers.
    RoomStateChanged(RoomView),
    /// Room reached the gameplay start state.
    SessionStarted {
        /// Canonical start frame.
        start_frame: u64,
        /// Optional future server-time start contract for v2 clients.
        scheduled_start: Option<ScheduledSessionStart>,
        /// Current room state.
        room: RoomView,
    },
    /// Room requested startup clock samples from v2 clients.
    ClockSyncSampleRequested {
        /// Clock-sample request sent to each connected client.
        request: crate::protocol::ClockSyncSampleRequest,
        /// Current room epoch.
        room_epoch: u64,
        /// Current session epoch.
        session_epoch: u64,
    },
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
        /// New-epoch synchronized deadline for protocol v5.
        scheduled_start: Option<ScheduledSessionStart>,
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
    /// Snapshot chunk should be relayed to subscribers.
    SnapshotChunk {
        /// Connection that supplied the snapshot chunk.
        source: ConnectionId,
        /// Room epoch the snapshot belongs to.
        room_epoch: u64,
        /// Session epoch the snapshot belongs to.
        session_epoch: u64,
        /// Validated snapshot chunk.
        chunk: SnapshotChunk,
    },
    /// Snapshot manifest should be relayed to subscribers.
    SnapshotComplete {
        /// Connection that supplied the snapshot manifest.
        source: ConnectionId,
        /// Room epoch the snapshot belongs to.
        room_epoch: u64,
        /// Session epoch the snapshot belongs to.
        session_epoch: u64,
        /// Validated snapshot manifest.
        manifest: SnapshotManifest,
    },
    /// Host should upload a large snapshot through the file relay.
    SnapshotFileRelayUploadGranted {
        /// Connection that requested the file relay.
        source: ConnectionId,
        /// Room epoch the snapshot belongs to.
        room_epoch: u64,
        /// Session epoch the snapshot belongs to.
        session_epoch: u64,
        /// Private upload grant for the source connection.
        grant: SnapshotFileRelayGrant,
    },
    /// Guest can download a large snapshot through the file relay.
    SnapshotFileRelayDownloadReady {
        /// Connection that should download the file.
        receiver: ConnectionId,
        /// Room epoch the snapshot belongs to.
        room_epoch: u64,
        /// Session epoch the snapshot belongs to.
        session_epoch: u64,
        /// Private download grant for the receiver connection.
        grant: SnapshotFileRelayGrant,
    },
    /// Host should upload a temporary ROM through the file relay.
    RomRelayUploadGranted {
        /// Connection that should upload the file.
        source: ConnectionId,
        /// Room epoch the transfer belongs to.
        room_epoch: u64,
        /// Session epoch the transfer belongs to.
        session_epoch: u64,
        /// Private upload grant for the source connection.
        grant: RomRelayGrant,
    },
    /// Guest can download a temporary ROM through the file relay.
    RomRelayDownloadGranted {
        /// Connection that should download the file.
        receiver: ConnectionId,
        /// Room epoch the transfer belongs to.
        room_epoch: u64,
        /// Session epoch the transfer belongs to.
        session_epoch: u64,
        /// Private download grant for the receiver connection.
        grant: RomRelayGrant,
    },
    /// Temporary ROM relay progress changed.
    RomRelayProgress {
        /// Connection that reported progress.
        source: ConnectionId,
        /// Room epoch the transfer belongs to.
        room_epoch: u64,
        /// Session epoch the transfer belongs to.
        session_epoch: u64,
        /// Progress payload.
        progress: RomRelayProgress,
    },
    /// Temporary ROM relay upload/download was verified by a client.
    RomRelayCompleted {
        /// Connection that reported completion.
        source: ConnectionId,
        /// Room epoch the transfer belongs to.
        room_epoch: u64,
        /// Session epoch the transfer belongs to.
        session_epoch: u64,
        /// Completion payload.
        completion: RomRelayCompletion,
    },
    /// Temporary ROM relay failed.
    RomRelayFailed {
        /// Connection that reported or caused failure.
        source: ConnectionId,
        /// Room epoch the transfer belongs to.
        room_epoch: u64,
        /// Session epoch the transfer belongs to.
        session_epoch: u64,
        /// Failure payload.
        failure: RomRelayFailure,
    },
    /// Temporary ROM relay was cancelled.
    RomRelayCancelled {
        /// Connection that requested cancellation.
        source: ConnectionId,
        /// Room epoch the transfer belongs to.
        room_epoch: u64,
        /// Session epoch the transfer belongs to.
        session_epoch: u64,
        /// Cancel payload.
        cancelled: RomRelayCancelled,
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
    /// Protocol v5 froze the old epoch while the host pins exact repair state.
    StateRecoveryPrepare {
        /// Preparing recovery transaction.
        recovery: StateRecoveryView,
        /// Current old-epoch room state.
        room: RoomView,
    },
    /// Protocol v5 committed a pinned repair snapshot to a fresh epoch.
    StateRecoveryCommitted {
        /// Committed recovery transaction.
        recovery: StateRecoveryView,
        /// Current fresh-epoch room state.
        room: RoomView,
    },
    /// Protocol v5 recovery could not pin state within its bounded window.
    StateRecoveryFailed {
        /// Last known recovery transaction.
        recovery: StateRecoveryView,
        /// Stable failure reason.
        reason: String,
        /// Closed room state.
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
    /// Validated fast-input record should be relayed without re-encoding.
    FastInputFrame {
        /// Connection that supplied the input record.
        source: ConnectionId,
        /// Validated self-contained fast-input record.
        frame: FastInputFrame,
    },
    /// Strict protocol v5 input should be relayed immediately to the peer.
    StrictInputBatch {
        /// Input socket that supplied the batch.
        source: ConnectionId,
        /// Newly accepted contiguous input suffix.
        batch: StrictInputBatch,
    },
    /// Canonical server frame released to every input socket.
    ServerFrame {
        /// Relay-owned frame release cursor.
        frame: ServerFrame,
    },
    /// Host-driven protocol v5 frame release sent to every input socket.
    ServerFrameV5 {
        /// Cumulative frame and accepted-input cursors.
        release: ServerFrameReleaseV5,
    },
}
