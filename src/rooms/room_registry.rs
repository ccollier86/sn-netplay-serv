//! In-memory registry for active netplay rooms.
//!
//! The registry owns invite-code lookup and room mutation synchronization. It
//! does not validate licenses or serialize HTTP responses directly.

use super::stored_room::StoredRoom;
use crate::auth::VerifiedLicense;
use crate::protocol::{
    ClientRuntimeState, CompatibilityFingerprint, InputFrame, InputFrameBatch,
    LinkCableCompatibility, LinkCablePacket, NetplaySessionDescriptor, SessionPauseReason,
    SnapshotChunk, SnapshotManifest,
};
use crate::rooms::{
    Clock, ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex, ResumeTokenGenerator,
    RoomDebugEvent, RoomDebugEventLog, RoomError, RoomEventReceiver, RoomJoin, RoomRecoveryConfig,
    RoomRegistry, RoomRegistrySnapshot, RoomView, SystemClock, UuidResumeTokenGenerator,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[path = "room_registry_lifecycle_ops.rs"]
mod lifecycle_ops;
#[path = "room_registry_query_ops.rs"]
mod query_ops;
#[path = "room_registry_relay_ops.rs"]
mod relay_ops;
#[path = "room_registry_sync_ops.rs"]
mod sync_ops;

/// Thread-safe in-memory room registry.
pub struct InMemoryRoomRegistry {
    invite_codes: RwLock<HashMap<String, StoredRoom>>,
    invite_code_generator: Arc<dyn InviteCodeGenerator>,
    resume_token_generator: Arc<dyn ResumeTokenGenerator>,
    clock: Arc<dyn Clock>,
    recovery_config: RoomRecoveryConfig,
    recent_events: Mutex<RoomDebugEventLog>,
}

impl InMemoryRoomRegistry {
    /// Creates an empty registry with the supplied invite-code generator.
    pub fn new(invite_code_generator: Arc<dyn InviteCodeGenerator>) -> Self {
        Self::with_dependencies(
            invite_code_generator,
            Arc::new(UuidResumeTokenGenerator),
            Arc::new(SystemClock),
            RoomRecoveryConfig::default(),
        )
    }

    /// Creates an empty registry with injectable lifecycle dependencies.
    pub fn with_dependencies(
        invite_code_generator: Arc<dyn InviteCodeGenerator>,
        resume_token_generator: Arc<dyn ResumeTokenGenerator>,
        clock: Arc<dyn Clock>,
        recovery_config: RoomRecoveryConfig,
    ) -> Self {
        Self {
            invite_codes: RwLock::new(HashMap::new()),
            invite_code_generator,
            resume_token_generator,
            clock,
            recovery_config,
            recent_events: Mutex::new(RoomDebugEventLog::default()),
        }
    }

    pub(super) fn record_recent_events(&self, events: Vec<RoomDebugEvent>) {
        let Ok(mut recent_events) = self.recent_events.lock() else {
            return;
        };

        for event in events {
            recent_events.push(event);
        }
    }

    /// Removes rooms still waiting for a guest after `join_timeout`.
    pub async fn remove_expired_waiting_rooms(
        &self,
        now: Instant,
        join_timeout: Duration,
    ) -> usize {
        self.sweep_expired_rooms(now, join_timeout).await
    }

    /// Test-facing pause helper that uses an empty idempotency key.
    pub async fn request_session_pause(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        reason: SessionPauseReason,
        local_frame: u64,
    ) -> Result<RoomView, RoomError> {
        self.request_session_pause_impl(
            invite_code,
            connection_id,
            String::new(),
            reason,
            local_frame,
        )
        .await
    }
}

#[async_trait::async_trait]
impl RoomRegistry for InMemoryRoomRegistry {
    async fn create_room(
        &self,
        host: VerifiedLicense,
        host_connection: ConnectionId,
        session: NetplaySessionDescriptor,
    ) -> Result<RoomView, RoomError> {
        self.create_room_impl(host, host_connection, session).await
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
    ) -> Result<RoomJoin, RoomError> {
        self.connect_host_impl(invite_code, host, connection_id)
            .await
    }

    async fn connect_guest(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<RoomJoin, RoomError> {
        self.connect_guest_impl(invite_code, guest, connection_id)
            .await
    }

    async fn reconnect_player(
        &self,
        invite_code: InviteCode,
        player_index: PlayerIndex,
        room_epoch: u64,
        resume_token: String,
        connection_id: ConnectionId,
    ) -> Result<RoomJoin, RoomError> {
        self.reconnect_player_impl(
            invite_code,
            player_index,
            room_epoch,
            resume_token,
            connection_id,
        )
        .await
    }

    async fn disconnect(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        self.disconnect_impl(invite_code, connection_id).await
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
    ) -> Result<RoomView, RoomError> {
        self.mark_ready_impl(invite_code, connection_id).await
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

    async fn relay_link_cable_packet(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        packet: LinkCablePacket,
    ) -> Result<(), RoomError> {
        self.relay_link_cable_packet_impl(invite_code, connection_id, packet)
            .await
    }

    async fn record_heartbeat(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        _latest_event_seq: u64,
        _local_frame: Option<u64>,
        runtime_state: ClientRuntimeState,
    ) -> Result<RoomView, RoomError> {
        self.record_heartbeat_impl(invite_code, connection_id, runtime_state)
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

#[cfg(test)]
#[path = "room_registry_link_tests.rs"]
mod room_registry_link_tests;

#[cfg(test)]
#[path = "room_registry_tests.rs"]
mod room_registry_tests;
