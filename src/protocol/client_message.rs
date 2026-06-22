//! Client-to-server WebSocket messages.
//!
//! These message types are transport payloads only. Domain validation still
//! happens in room modules before input or state is accepted.

use crate::protocol::{
    ClientNetworkQualityReport, ClientRuntimeState, ClockSyncPing, ClockSyncSample,
    CompatibilityFingerprint, DeterministicReadyReport, InputFrame, LinkCableCompatibility,
    LinkCablePacket, RomRelayCancelled as RomRelayCancelledPayload,
    RomRelayCompletion as RomRelayCompletionPayload, RomRelayFailure as RomRelayFailurePayload,
    RomRelayProgress as RomRelayProgressPayload, SessionPauseReason, SnapshotChunk,
    SnapshotManifest, StateHashReport,
};
use serde::Deserialize;

/// Message sent by a Desktop client over a room WebSocket.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ClientMessage {
    /// Lightweight connection keepalive.
    Ping,
    /// Client-originated clock ping for diagnostics and clock estimates.
    ClockSyncPing {
        /// Clock ping payload.
        ping: ClockSyncPing,
    },
    /// Client response to a server-requested startup clock sample.
    ClockSyncSample {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Clock sample payload.
        sample: ClockSyncSample,
    },
    /// Client compatibility fingerprint for the current game/core.
    SetCompatibilityFingerprint {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Netplay-relevant compatibility details.
        fingerprint: CompatibilityFingerprint,
    },
    /// Client link-cable compatibility for the selected runtime.
    SetLinkCableCompatibility {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Link-cable runtime compatibility details.
        compatibility: LinkCableCompatibility,
    },
    /// Client is ready to start or continue the sync phase.
    Ready {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Latest network/runtime health sample, when the client has one.
        #[serde(default)]
        network: Option<ClientNetworkQualityReport>,
    },
    /// Client reports its runner is ready for synchronized frame release.
    DeterministicReady {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Deterministic readiness payload.
        report: DeterministicReadyReport,
        /// Latest network/runtime health sample, when the client has one.
        #[serde(default)]
        network: Option<ClientNetworkQualityReport>,
    },
    /// One chunk of host save-state snapshot data.
    SnapshotChunk {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Chunk payload.
        chunk: SnapshotChunk,
    },
    /// Manifest for a completed snapshot transfer.
    SnapshotComplete {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Snapshot manifest.
        manifest: SnapshotManifest,
    },
    /// Host asks the relay to create a temporary file-relay upload for a large
    /// save-state snapshot.
    SnapshotFileRelayRequested {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Snapshot manifest.
        manifest: SnapshotManifest,
    },
    /// Host finished uploading a previously granted file-relay snapshot.
    SnapshotFileRelayUploadComplete {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// File relay transfer id.
        transfer_id: String,
        /// Snapshot manifest.
        manifest: SnapshotManifest,
    },
    /// Guest requests temporary ROM relay for this direct-invite room.
    #[serde(rename = "romRelay.request")]
    RomRelayRequest {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
    },
    /// Client reports upload/download progress for a temporary ROM relay.
    #[serde(rename = "romRelay.progress")]
    RomRelayProgress {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Progress payload.
        progress: RomRelayProgressPayload,
    },
    /// Client reports a verified upload/download completion.
    #[serde(rename = "romRelay.completed")]
    RomRelayCompleted {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Completion payload.
        completion: RomRelayCompletionPayload,
    },
    /// Client reports a transfer failure.
    #[serde(rename = "romRelay.failed")]
    RomRelayFailed {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Failure payload.
        failure: RomRelayFailurePayload,
    },
    /// Client cancels a temporary ROM relay transfer.
    #[serde(rename = "romRelay.cancelled")]
    RomRelayCancelled {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Cancel payload.
        cancelled: RomRelayCancelledPayload,
    },
    /// Frame-numbered input from the local player.
    InputFrame {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Normalized input payload.
        input: InputFrame,
    },
    /// Opaque virtual link-cable packet from the local runtime.
    LinkCablePacket {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Link packet to relay.
        packet: LinkCablePacket,
    },
    /// Periodic client health and progress report.
    Heartbeat {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Latest server event sequence applied locally.
        latest_event_seq: u64,
        /// Local runtime frame when available.
        local_frame: Option<u64>,
        /// Local emulator/netplay runtime state.
        runtime_state: ClientRuntimeState,
        /// Latest network/runtime health sample, when the client has one.
        #[serde(default)]
        network: Option<ClientNetworkQualityReport>,
    },
    /// Request a room-wide pause at a relay-selected frame.
    RequestSessionPause {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Client-generated id for logs and idempotent UI actions.
        request_id: String,
        /// Reason the client is pausing.
        reason: SessionPauseReason,
        /// Client's local frame when it requested the pause.
        local_frame: u64,
    },
    /// A client reached and paused at the scheduled frame.
    SessionPauseReached {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Pause sequence being acknowledged.
        sequence: u64,
        /// Frame where the runtime actually paused.
        paused_at_frame: u64,
    },
    /// Release this client's pause holder and resume if every holder is gone.
    RequestSessionResume {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Client-generated id for logs and idempotent UI actions.
        request_id: String,
        /// Pause sequence being released.
        sequence: u64,
        /// Reason being released.
        reason: SessionPauseReason,
    },
    /// Client is intentionally leaving the active room.
    PlayerExited {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Short client-provided reason for diagnostics and peer UI.
        reason: String,
    },
    /// Client requests a fresh private voice-room token.
    RefreshVoiceToken {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
    },
    /// Low-frequency deterministic state hash for desync detection.
    StateHash {
        /// Current room epoch observed by the client.
        room_epoch: u64,
        /// Current session epoch observed by the client.
        session_epoch: u64,
        /// Hash report for one emulator frame.
        report: StateHashReport,
    },
}
