//! Room registry interface used by HTTP and WebSocket transports.
//!
//! The trait keeps transports depending on room capabilities instead of the
//! concrete in-memory implementation. It owns no locking, serialization, or
//! authorization behavior.

use crate::auth::VerifiedLicense;
use crate::protocol::{
    ClientRuntimeState, CompatibilityFingerprint, FastInputBatch, InputFrame, InputFrameBatch,
    LinkCableCompatibility, LinkCablePacket, NetplaySessionDescriptor, RomRelayBlockReason,
    RomRelayCancelled, RomRelayCompletion, RomRelayFailure, RomRelayProgress, SessionPauseReason,
    SnapshotChunk, SnapshotFileRelayGrantPair, SnapshotManifest, StateHashReport,
};
use crate::rooms::{
    ClientTransportCapabilities, ConnectionId, InviteCode, PlayerIndex, RomRelayGrantPair,
    RomRelayTransferIntent, RoomDebugEvent, RoomError, RoomEvent, RoomInputEvent, RoomJoin,
    RoomRegistrySnapshot, RoomView, RoomVoiceTokenRefresh, SnapshotFileRelayTransferIntent,
};
use tokio::sync::broadcast;

/// Receiver for room domain events.
pub type RoomEventReceiver = broadcast::Receiver<RoomEvent>;
/// Receiver for dedicated gameplay input events.
pub type RoomInputEventReceiver = broadcast::Receiver<RoomInputEvent>;

/// Room storage behavior needed by transports and routes.
#[async_trait::async_trait]
pub trait RoomRegistry: Send + Sync {
    /// Returns monotonic server milliseconds for protocol clock samples.
    fn server_time_ms(&self) -> u64;

    /// Creates a new room for a verified host.
    async fn create_room(
        &self,
        host: VerifiedLicense,
        host_connection: ConnectionId,
        session: NetplaySessionDescriptor,
    ) -> Result<RoomView, RoomError>;

    /// Adds a verified guest to an existing room.
    async fn join_guest(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError>;

    /// Attaches a host socket to its reserved Player 1 slot.
    async fn connect_host(
        &self,
        invite_code: InviteCode,
        host: VerifiedLicense,
        connection_id: ConnectionId,
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError>;

    /// Adds a verified guest socket and returns the joined room state.
    async fn connect_guest(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError>;

    /// Reclaims an occupied player slot with a valid resume token.
    async fn reconnect_player(
        &self,
        invite_code: InviteCode,
        player_index: PlayerIndex,
        room_epoch: u64,
        resume_token: String,
        connection_id: ConnectionId,
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError>;

    /// Arms an authenticated initial control join for a short-lived runner takeover.
    async fn arm_runner_handoff(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<(), RoomError>;

    /// Cancels a runner handoff whose capability could not be delivered.
    async fn cancel_runner_handoff(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<(), RoomError>;

    /// Marks a socket connection as disconnected.
    async fn disconnect(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError>;

    /// Records a sanitized transport close/error before room lifecycle cleanup.
    async fn record_transport_close(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        socket_kind: &'static str,
        reason: String,
    ) -> Result<(), RoomError>;

    /// Ends a room because one player intentionally left.
    async fn player_exited(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        reason: String,
    ) -> Result<RoomView, RoomError>;

    /// Refreshes the private voice token for one connected player.
    async fn refresh_voice_token(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomVoiceTokenRefresh, RoomError>;

    /// Attaches a binary input socket to an occupied player slot.
    async fn connect_input_socket(
        &self,
        invite_code: InviteCode,
        player_index: PlayerIndex,
        room_epoch: u64,
        session_epoch: u64,
        input_socket_token: String,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError>;

    /// Detaches a binary input socket and starts recovery when needed.
    async fn disconnect_input_socket(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError>;

    /// Subscribes to domain events for one active room.
    async fn subscribe(&self, invite_code: InviteCode) -> Result<RoomEventReceiver, RoomError>;

    /// Subscribes to gameplay input events for one active room.
    async fn subscribe_input(
        &self,
        invite_code: InviteCode,
    ) -> Result<RoomInputEventReceiver, RoomError>;

    /// Stores a compatibility fingerprint from one connected player.
    async fn set_compatibility(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        fingerprint: CompatibilityFingerprint,
    ) -> Result<RoomView, RoomError>;

    /// Stores link-cable compatibility details from one connected player.
    async fn set_link_cable_compatibility(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        compatibility: LinkCableCompatibility,
    ) -> Result<RoomView, RoomError>;

    /// Marks one connected player ready and starts the session when all are ready.
    async fn mark_ready(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        network: Option<crate::protocol::ClientNetworkQualityReport>,
    ) -> Result<RoomView, RoomError>;

    /// Records one v2 startup clock sample.
    async fn record_clock_sync_sample(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        sample: crate::protocol::ClockSyncSample,
    ) -> Result<RoomView, RoomError>;

    /// Marks one v2 client deterministic-ready for scheduled release.
    async fn mark_deterministic_ready(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        report: crate::protocol::DeterministicReadyReport,
        network: Option<crate::protocol::ClientNetworkQualityReport>,
    ) -> Result<RoomView, RoomError>;

    /// Validates and broadcasts a host snapshot chunk.
    async fn relay_snapshot_chunk(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        chunk: SnapshotChunk,
    ) -> Result<(), RoomError>;

    /// Validates and broadcasts a host snapshot completion manifest.
    async fn relay_snapshot_complete(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
    ) -> Result<(), RoomError>;

    /// Validates a host request for large snapshot file relay.
    async fn prepare_snapshot_file_relay(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
    ) -> Result<SnapshotFileRelayTransferIntent, RoomError>;

    /// Stores file-relay grants and privately grants the host upload.
    async fn grant_snapshot_file_relay_upload(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
        grant_pair: SnapshotFileRelayGrantPair,
    ) -> Result<(), RoomError>;

    /// Completes a file-relay upload and privately grants the guest download.
    async fn relay_snapshot_file_upload_complete(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        transfer_id: String,
        manifest: SnapshotManifest,
    ) -> Result<(), RoomError>;

    /// Validates a guest request for direct-invite temporary ROM relay.
    async fn prepare_rom_relay(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RomRelayTransferIntent, RomRelayBlockReason>;

    /// Stores ROM relay grants and privately grants the host upload.
    async fn grant_rom_relay_upload(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        grants: RomRelayGrantPair,
    ) -> Result<(), RomRelayBlockReason>;

    /// Validates and emits a ROM relay progress update.
    async fn relay_rom_relay_progress(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        progress: RomRelayProgress,
    ) -> Result<(), RomRelayBlockReason>;

    /// Validates and emits ROM relay upload/download completion.
    async fn relay_rom_relay_completed(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        completion: RomRelayCompletion,
    ) -> Result<(), RomRelayBlockReason>;

    /// Validates and emits a ROM relay failure.
    async fn relay_rom_relay_failed(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        failure: RomRelayFailure,
    ) -> Result<(), RomRelayBlockReason>;

    /// Validates and emits a ROM relay cancellation.
    async fn relay_rom_relay_cancelled(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        cancelled: RomRelayCancelled,
    ) -> Result<(), RomRelayBlockReason>;

    /// Validates and broadcasts one frame of player input.
    async fn relay_input_frame(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        input: InputFrame,
    ) -> Result<(), RoomError>;

    /// Validates and broadcasts a binary batch of player input frames.
    async fn relay_input_frame_batch(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: InputFrameBatch,
    ) -> Result<(), RoomError>;

    /// Validates and relays fast binary input records.
    async fn relay_fast_input_batch(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: FastInputBatch,
    ) -> Result<(), RoomError>;

    /// Validates and broadcasts one virtual link-cable packet.
    async fn relay_link_cable_packet(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        packet: LinkCablePacket,
    ) -> Result<(), RoomError>;

    /// Records a deterministic state hash report.
    async fn record_state_hash(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        report: StateHashReport,
    ) -> Result<(), RoomError>;

    /// Records a client heartbeat and returns the current room view.
    async fn record_heartbeat(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        latest_event_seq: u64,
        local_frame: Option<u64>,
        runtime_state: ClientRuntimeState,
        network: Option<crate::protocol::ClientNetworkQualityReport>,
    ) -> Result<RoomView, RoomError>;

    /// Requests a coordinated room pause.
    async fn request_session_pause(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        request_id: String,
        reason: SessionPauseReason,
        local_frame: u64,
    ) -> Result<RoomView, RoomError>;

    /// Marks one client paused at the scheduled frame.
    async fn mark_session_pause_reached(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        sequence: u64,
        paused_at_frame: u64,
    ) -> Result<RoomView, RoomError>;

    /// Releases one client's coordinated pause holder.
    async fn request_session_resume(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        request_id: String,
        reason: SessionPauseReason,
        sequence: u64,
    ) -> Result<RoomView, RoomError>;

    /// Returns a serializable room view for an invite code.
    async fn room_view(&self, invite_code: InviteCode) -> Result<RoomView, RoomError>;

    /// Returns sanitized events for one room.
    async fn room_events(
        &self,
        invite_code: InviteCode,
        limit: usize,
    ) -> Result<Vec<RoomDebugEvent>, RoomError>;

    /// Returns sanitized recent events across active rooms.
    async fn recent_events(&self, limit: usize) -> Vec<RoomDebugEvent>;

    /// Returns a point-in-time snapshot of active rooms.
    async fn snapshot(&self) -> RoomRegistrySnapshot;
}
