//! RoomRegistry trait adapter for the in-memory registry.

use super::InMemoryRoomRegistry;
use crate::auth::VerifiedLicense;
use crate::protocol::{
    ClientRuntimeState, CompatibilityFingerprint, FastInputBatch, HostFrameOpen,
    InputCursorResponse, InputFrame, InputFrameBatch, LinkCableCompatibility, LinkCablePacket,
    NetplaySessionDescriptor, RomRelayBlockReason, RomRelayCancelled, RomRelayCompletion,
    RomRelayFailure, RomRelayProgress, SessionPauseReason, SnapshotChunk,
    SnapshotFileRelayGrantPair, SnapshotManifest, StateHashReport, StrictInputBatch,
};
use crate::rooms::{
    ClientTransportCapabilities, ConnectionId, HostFrameRelayOutcome, InviteCode, PlayerIndex,
    RomRelayGrantPair, RomRelayTransferIntent, RoomDebugEvent, RoomError, RoomEventReceiver,
    RoomJoin, RoomRegistry, RoomRegistrySnapshot, RoomView, SnapshotFileRelayTransferIntent,
};

#[async_trait::async_trait]
impl RoomRegistry for InMemoryRoomRegistry {
    fn server_time_ms(&self) -> u64 {
        InMemoryRoomRegistry::server_time_ms(self)
    }

    async fn create_room(
        &self,
        host: VerifiedLicense,
        host_connection: ConnectionId,
        session: NetplaySessionDescriptor,
    ) -> Result<RoomView, RoomError> {
        self.create_room_impl(
            host,
            host_connection,
            session,
            crate::protocol::LEGACY_NETPLAY_PROTOCOL_VERSION,
        )
        .await
    }

    async fn create_room_with_protocol(
        &self,
        host: VerifiedLicense,
        host_connection: ConnectionId,
        session: NetplaySessionDescriptor,
        protocol_version: u16,
    ) -> Result<RoomView, RoomError> {
        self.create_room_impl(host, host_connection, session, protocol_version)
            .await
    }

    async fn join_guest(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        self.join_guest_impl(invite_code, guest, connection_id)
            .await
    }

    async fn connect_host(
        &self,
        invite_code: InviteCode,
        host: VerifiedLicense,
        connection_id: ConnectionId,
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError> {
        self.connect_host_impl(invite_code, host, connection_id, capabilities)
            .await
    }

    async fn connect_guest(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError> {
        self.connect_guest_impl(invite_code, guest, connection_id, capabilities)
            .await
    }

    async fn reconnect_player(
        &self,
        invite_code: InviteCode,
        player_index: PlayerIndex,
        room_epoch: u64,
        resume_token: String,
        connection_id: ConnectionId,
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError> {
        self.reconnect_player_impl(
            invite_code,
            player_index,
            room_epoch,
            resume_token,
            connection_id,
            capabilities,
        )
        .await
    }

    async fn arm_runner_handoff(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<(), RoomError> {
        self.arm_runner_handoff_impl(invite_code, connection_id)
            .await
    }

    async fn cancel_runner_handoff(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<(), RoomError> {
        self.cancel_runner_handoff_impl(invite_code, connection_id)
            .await
    }

    async fn disconnect(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        self.disconnect_impl(invite_code, connection_id).await
    }

    async fn record_transport_close(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        socket_kind: &'static str,
        reason: String,
    ) -> Result<(), RoomError> {
        self.record_transport_close_impl(invite_code, connection_id, socket_kind, reason)
            .await
    }

    async fn player_exited(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        reason: String,
    ) -> Result<RoomView, RoomError> {
        self.player_exited_impl(invite_code, connection_id, reason)
            .await
    }

    async fn refresh_voice_token(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<crate::rooms::RoomVoiceTokenRefresh, RoomError> {
        self.refresh_voice_token_impl(invite_code, connection_id)
            .await
    }

    async fn connect_input_socket(
        &self,
        invite_code: InviteCode,
        player_index: PlayerIndex,
        room_epoch: u64,
        session_epoch: u64,
        input_socket_token: String,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        self.connect_input_socket_impl(
            invite_code,
            player_index,
            room_epoch,
            session_epoch,
            input_socket_token,
            connection_id,
        )
        .await
    }

    async fn disconnect_input_socket(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        self.disconnect_input_socket_impl(invite_code, connection_id)
            .await
    }

    async fn subscribe(&self, invite_code: InviteCode) -> Result<RoomEventReceiver, RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        Ok(stored_room.subscribe())
    }

    async fn subscribe_input(
        &self,
        invite_code: InviteCode,
    ) -> Result<crate::rooms::RoomInputEventReceiver, RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        Ok(stored_room.subscribe_input())
    }

    async fn set_compatibility(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        fingerprint: CompatibilityFingerprint,
    ) -> Result<RoomView, RoomError> {
        self.set_compatibility_impl(invite_code, connection_id, fingerprint)
            .await
    }

    async fn mark_ready(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        network: Option<crate::protocol::ClientNetworkQualityReport>,
    ) -> Result<RoomView, RoomError> {
        self.mark_ready_impl(invite_code, connection_id, network)
            .await
    }

    async fn record_clock_sync_sample(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        sample: crate::protocol::ClockSyncSample,
    ) -> Result<RoomView, RoomError> {
        self.record_clock_sync_sample_impl(invite_code, connection_id, sample)
            .await
    }

    async fn mark_deterministic_ready(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        report: crate::protocol::DeterministicReadyReport,
        network: Option<crate::protocol::ClientNetworkQualityReport>,
    ) -> Result<RoomView, RoomError> {
        self.mark_deterministic_ready_impl(invite_code, connection_id, report, network)
            .await
    }

    async fn set_link_cable_compatibility(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        compatibility: LinkCableCompatibility,
    ) -> Result<RoomView, RoomError> {
        self.set_link_cable_compatibility_impl(invite_code, connection_id, compatibility)
            .await
    }

    async fn relay_snapshot_chunk(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        chunk: SnapshotChunk,
    ) -> Result<(), RoomError> {
        self.relay_snapshot_chunk_impl(invite_code, connection_id, chunk)
            .await
    }

    async fn relay_snapshot_complete(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
    ) -> Result<(), RoomError> {
        self.relay_snapshot_complete_impl(invite_code, connection_id, manifest)
            .await
    }

    async fn prepare_snapshot_file_relay(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
    ) -> Result<SnapshotFileRelayTransferIntent, RoomError> {
        self.prepare_snapshot_file_relay_impl(invite_code, connection_id, manifest)
            .await
    }

    async fn grant_snapshot_file_relay_upload(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
        grant_pair: SnapshotFileRelayGrantPair,
    ) -> Result<(), RoomError> {
        self.grant_snapshot_file_relay_upload_impl(invite_code, connection_id, manifest, grant_pair)
            .await
    }

    async fn relay_snapshot_file_upload_complete(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        transfer_id: String,
        manifest: SnapshotManifest,
    ) -> Result<(), RoomError> {
        self.relay_snapshot_file_upload_complete_impl(
            invite_code,
            connection_id,
            transfer_id,
            manifest,
        )
        .await
    }

    async fn prepare_rom_relay(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RomRelayTransferIntent, RomRelayBlockReason> {
        self.prepare_rom_relay_impl(invite_code, connection_id)
            .await
    }

    async fn grant_rom_relay_upload(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        grants: RomRelayGrantPair,
    ) -> Result<(), RomRelayBlockReason> {
        self.grant_rom_relay_upload_impl(invite_code, connection_id, grants)
            .await
    }

    async fn relay_rom_relay_progress(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        progress: RomRelayProgress,
    ) -> Result<(), RomRelayBlockReason> {
        self.relay_rom_relay_progress_impl(invite_code, connection_id, progress)
            .await
    }

    async fn relay_rom_relay_completed(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        completion: RomRelayCompletion,
    ) -> Result<(), RomRelayBlockReason> {
        self.relay_rom_relay_completed_impl(invite_code, connection_id, completion)
            .await
    }

    async fn relay_rom_relay_failed(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        failure: RomRelayFailure,
    ) -> Result<(), RomRelayBlockReason> {
        self.relay_rom_relay_failed_impl(invite_code, connection_id, failure)
            .await
    }

    async fn relay_rom_relay_cancelled(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        cancelled: RomRelayCancelled,
    ) -> Result<(), RomRelayBlockReason> {
        self.relay_rom_relay_cancelled_impl(invite_code, connection_id, cancelled)
            .await
    }

    async fn relay_input_frame(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        input: InputFrame,
    ) -> Result<(), RoomError> {
        self.relay_input_frame_impl(invite_code, connection_id, input)
            .await
    }

    async fn relay_input_frame_batch(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: InputFrameBatch,
    ) -> Result<(), RoomError> {
        self.relay_input_frame_batch_impl(invite_code, connection_id, batch)
            .await
    }

    async fn relay_fast_input_batch(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: FastInputBatch,
    ) -> Result<(), RoomError> {
        self.relay_fast_input_batch_impl(invite_code, connection_id, batch)
            .await
    }

    async fn relay_strict_input_batch(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        batch: StrictInputBatch,
    ) -> Result<InputCursorResponse, RoomError> {
        self.relay_strict_input_batch_impl(invite_code, connection_id, batch)
            .await
    }

    async fn relay_host_frame_open(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        open: HostFrameOpen,
    ) -> Result<HostFrameRelayOutcome, RoomError> {
        self.relay_host_frame_open_impl(invite_code, connection_id, open)
            .await
    }

    async fn release_scheduled_v5_host_frame(
        &self,
        invite_code: InviteCode,
        room_epoch: u64,
        session_epoch: u64,
        frame: u64,
    ) -> Result<(), RoomError> {
        self.release_scheduled_v5_host_frame_impl(invite_code, room_epoch, session_epoch, frame)
            .await
    }

    async fn relay_link_cable_packet(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        packet: LinkCablePacket,
    ) -> Result<(), RoomError> {
        self.relay_link_cable_packet_impl(invite_code, connection_id, packet)
            .await
    }

    async fn record_state_hash(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        report: StateHashReport,
    ) -> Result<(), RoomError> {
        self.record_state_hash_impl(invite_code, connection_id, report)
            .await
    }

    async fn record_heartbeat(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        _latest_event_seq: u64,
        _local_frame: Option<u64>,
        runtime_state: ClientRuntimeState,
        network: Option<crate::protocol::ClientNetworkQualityReport>,
    ) -> Result<RoomView, RoomError> {
        self.record_heartbeat_impl(
            invite_code,
            connection_id,
            _local_frame,
            network,
            runtime_state,
        )
        .await
    }

    async fn request_session_pause(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        request_id: String,
        reason: SessionPauseReason,
        local_frame: u64,
    ) -> Result<RoomView, RoomError> {
        self.request_session_pause_impl(invite_code, connection_id, request_id, reason, local_frame)
            .await
    }

    async fn mark_session_pause_reached(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        sequence: u64,
        paused_at_frame: u64,
    ) -> Result<RoomView, RoomError> {
        self.mark_session_pause_reached_impl(invite_code, connection_id, sequence, paused_at_frame)
            .await
    }

    async fn request_session_resume(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        request_id: String,
        reason: SessionPauseReason,
        sequence: u64,
    ) -> Result<RoomView, RoomError> {
        self.request_session_resume_impl(invite_code, connection_id, request_id, reason, sequence)
            .await
    }

    async fn room_view(&self, invite_code: InviteCode) -> Result<RoomView, RoomError> {
        self.room_view_impl(invite_code).await
    }

    async fn room_events(
        &self,
        invite_code: InviteCode,
        limit: usize,
    ) -> Result<Vec<RoomDebugEvent>, RoomError> {
        self.room_events_impl(invite_code, limit).await
    }

    async fn recent_events(&self, limit: usize) -> Vec<RoomDebugEvent> {
        self.recent_events_impl(limit).await
    }

    async fn snapshot(&self) -> RoomRegistrySnapshot {
        self.snapshot_impl().await
    }
}
