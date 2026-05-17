//! In-memory registry for active netplay rooms.
//!
//! The registry owns invite-code lookup and room mutation synchronization. It
//! does not validate licenses or serialize HTTP responses directly.

use crate::auth::VerifiedLicense;
use crate::protocol::{
    CompatibilityFingerprint, InputFrame, InputFrameLimits, SnapshotChunk, SnapshotLimits,
    SnapshotManifest,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, NetplayRoom, PlayerIndex, RoomError, RoomEvent,
    RoomRegistrySnapshot, RoomView,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast};

const ROOM_EVENT_CHANNEL_CAPACITY: usize = 32;

/// Room storage behavior needed by transports and routes.
#[async_trait::async_trait]
pub trait RoomRegistry: Send + Sync {
    /// Creates a new room for a verified host.
    async fn create_room(
        &self,
        host: VerifiedLicense,
        host_connection: ConnectionId,
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
    ) -> Result<RoomJoin, RoomError>;

    /// Adds a verified guest socket and returns the joined room state.
    async fn connect_guest(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<RoomJoin, RoomError>;

    /// Marks a socket connection as disconnected.
    async fn disconnect(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError>;

    /// Subscribes to domain events for one active room.
    async fn subscribe(&self, invite_code: InviteCode) -> Result<RoomEventReceiver, RoomError>;

    /// Stores a compatibility fingerprint from one connected player.
    async fn set_compatibility(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        fingerprint: CompatibilityFingerprint,
    ) -> Result<RoomView, RoomError>;

    /// Marks one connected player ready and starts the session when all are ready.
    async fn mark_ready(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
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

    /// Validates and broadcasts one frame of player input.
    async fn relay_input_frame(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        input: InputFrame,
    ) -> Result<(), RoomError>;

    /// Returns a serializable room view for an invite code.
    async fn room_view(&self, invite_code: InviteCode) -> Result<RoomView, RoomError>;

    /// Returns a point-in-time snapshot of active rooms.
    async fn snapshot(&self) -> RoomRegistrySnapshot;
}

/// Thread-safe in-memory room registry.
pub struct InMemoryRoomRegistry {
    invite_codes: RwLock<HashMap<String, StoredRoom>>,
    invite_code_generator: Arc<dyn InviteCodeGenerator>,
}

impl InMemoryRoomRegistry {
    /// Creates an empty registry with the supplied invite-code generator.
    pub fn new(invite_code_generator: Arc<dyn InviteCodeGenerator>) -> Self {
        Self {
            invite_codes: RwLock::new(HashMap::new()),
            invite_code_generator,
        }
    }

    /// Removes rooms still waiting for a guest after `join_timeout`.
    pub async fn remove_expired_waiting_rooms(
        &self,
        now: Instant,
        join_timeout: Duration,
    ) -> usize {
        let mut rooms = self.invite_codes.write().await;
        let before_count = rooms.len();

        rooms.retain(|_, stored_room| !stored_room.is_expired_waiting(now, join_timeout));

        before_count.saturating_sub(rooms.len())
    }
}

/// Result returned when a socket joins a room.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomJoin {
    /// Player index assigned to the socket connection.
    pub player_index: PlayerIndex,
    /// Room state immediately after the join.
    pub room: RoomView,
}

/// Receiver for room domain events.
pub type RoomEventReceiver = broadcast::Receiver<RoomEvent>;

struct StoredRoom {
    room: NetplayRoom,
    events: broadcast::Sender<RoomEvent>,
    created_at: Instant,
}

impl StoredRoom {
    fn new(room: NetplayRoom) -> Self {
        let (events, _) = broadcast::channel(ROOM_EVENT_CHANNEL_CAPACITY);

        Self {
            room,
            events,
            created_at: Instant::now(),
        }
    }

    fn is_expired_waiting(&self, now: Instant, timeout: Duration) -> bool {
        self.room.status() == crate::rooms::RoomStatus::WaitingForGuest
            && now.duration_since(self.created_at) >= timeout
    }

    fn emit_state(&self) {
        let _ = self
            .events
            .send(RoomEvent::RoomStateChanged(self.room.view()));
    }

    fn emit_start(&self, start_frame: u64) {
        let _ = self.events.send(RoomEvent::SessionStarted {
            start_frame,
            room: self.room.view(),
        });
    }

    fn emit_snapshot_chunk(&self, source: ConnectionId, chunk: SnapshotChunk) {
        let _ = self.events.send(RoomEvent::SnapshotChunk { source, chunk });
    }

    fn emit_snapshot_complete(&self, source: ConnectionId, manifest: SnapshotManifest) {
        let _ = self
            .events
            .send(RoomEvent::SnapshotComplete { source, manifest });
    }

    fn emit_input_frame(&self, source: ConnectionId, input: InputFrame) {
        let _ = self.events.send(RoomEvent::InputFrame { source, input });
    }
}

#[async_trait::async_trait]
impl RoomRegistry for InMemoryRoomRegistry {
    async fn create_room(
        &self,
        host: VerifiedLicense,
        host_connection: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        let invite_code = self.invite_code_generator.generate();
        let room = NetplayRoom::new(host, host_connection, invite_code.clone());
        let view = room.view();

        self.invite_codes
            .write()
            .await
            .insert(invite_code.normalized().to_string(), StoredRoom::new(room));

        Ok(view)
    }

    async fn join_guest(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let player_index = stored_room.room.join_guest(guest, connection_id)?;

        stored_room.emit_state();

        Ok(player_index)
    }

    async fn connect_host(
        &self,
        invite_code: InviteCode,
        host: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<RoomJoin, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let player_index = stored_room.room.attach_host(host, connection_id)?;
        let room = stored_room.room.view();

        stored_room.emit_state();

        Ok(RoomJoin { player_index, room })
    }

    async fn connect_guest(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<RoomJoin, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let player_index = stored_room.room.join_guest(guest, connection_id)?;
        let room = stored_room.room.view();

        stored_room.emit_state();

        Ok(RoomJoin { player_index, room })
    }

    async fn disconnect(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        let _closed = stored_room.room.disconnect(connection_id)?;
        let room = stored_room.room.view();
        stored_room.emit_state();

        Ok(room)
    }

    async fn subscribe(&self, invite_code: InviteCode) -> Result<RoomEventReceiver, RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        Ok(stored_room.events.subscribe())
    }

    async fn set_compatibility(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        fingerprint: CompatibilityFingerprint,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room
            .room
            .set_compatibility_for_connection(connection_id, fingerprint)?;
        let room = stored_room.room.view();
        stored_room.emit_state();

        Ok(room)
    }

    async fn mark_ready(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let started = stored_room.room.mark_ready(connection_id)?;
        let room = stored_room.room.view();

        if started {
            stored_room.emit_start(0);
        } else {
            stored_room.emit_state();
        }

        Ok(room)
    }

    async fn relay_snapshot_chunk(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        chunk: SnapshotChunk,
    ) -> Result<(), RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room.room.validate_snapshot_chunk(
            connection_id,
            &chunk,
            SnapshotLimits::default(),
        )?;
        stored_room.emit_snapshot_chunk(connection_id, chunk);

        Ok(())
    }

    async fn relay_snapshot_complete(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
    ) -> Result<(), RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room.room.validate_snapshot_complete(
            connection_id,
            &manifest,
            SnapshotLimits::default(),
        )?;
        stored_room.emit_snapshot_complete(connection_id, manifest);

        Ok(())
    }

    async fn relay_input_frame(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        input: InputFrame,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room
            .room
            .accept_input_frame(connection_id, &input, InputFrameLimits::default())?;
        stored_room.emit_input_frame(connection_id, input);

        Ok(())
    }

    async fn room_view(&self, invite_code: InviteCode) -> Result<RoomView, RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        Ok(stored_room.room.view())
    }

    async fn snapshot(&self) -> RoomRegistrySnapshot {
        let rooms = self.invite_codes.read().await;
        let views = rooms
            .values()
            .map(|stored_room| stored_room.room.view())
            .collect::<Vec<_>>();

        RoomRegistrySnapshot {
            active_room_count: views.len(),
            rooms: views,
        }
    }
}

#[cfg(test)]
#[path = "room_registry_tests.rs"]
mod room_registry_tests;
