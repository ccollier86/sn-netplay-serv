//! Lifecycle helpers for the in-memory room registry.
//!
//! This module owns room creation, socket attachment, resume-token recovery,
//! and abandoned-room cleanup. It keeps lifecycle mutation out of the trait
//! adapter in `room_registry`.

use super::InMemoryRoomRegistry;
use crate::auth::VerifiedLicense;
use crate::protocol::NetplaySessionDescriptor;
use crate::rooms::stored_room::StoredRoom;
use crate::rooms::{
    ConnectionId, InviteCode, NetplayRoom, PlayerIndex, RoomError, RoomJoin, RoomView,
    hash_resume_token,
};
use std::time::{Duration, Instant};

impl InMemoryRoomRegistry {
    /// Creates a room and reserves Player 1 for the host.
    pub(super) async fn create_room_impl(
        &self,
        host: VerifiedLicense,
        host_connection: ConnectionId,
        session: NetplaySessionDescriptor,
    ) -> Result<RoomView, RoomError> {
        let invite_code = self.invite_code_generator.generate();
        let resume_token = self.resume_token_generator.generate();
        let now = self.clock.now();
        let room = NetplayRoom::new_with_resume(
            host,
            host_connection,
            invite_code.clone(),
            session,
            resume_token.hash(),
            now,
        );
        let view = room.view_for_event(0, now);

        self.invite_codes
            .write()
            .await
            .insert(invite_code.normalized().to_string(), StoredRoom::new(room));

        Ok(view)
    }

    /// Adds a guest without returning a resume token.
    pub(super) async fn join_guest_impl(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let resume_token = self.resume_token_generator.generate();
        let now = self.clock.now();
        let player_index = stored_room.room.join_guest_with_resume(
            guest,
            connection_id,
            resume_token.hash(),
            now,
        )?;

        stored_room.emit_state(now, "guestJoined", "guest joined room");
        self.record_recent_events(stored_room.debug_events(1));

        Ok(player_index)
    }

    /// Attaches the room creator socket to Player 1.
    pub(super) async fn connect_host_impl(
        &self,
        invite_code: InviteCode,
        host: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<RoomJoin, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let resume_token = self.resume_token_generator.generate();
        let now = self.clock.now();
        let player_index = stored_room.room.attach_host_with_resume(
            host,
            connection_id,
            resume_token.hash(),
            now,
        )?;

        stored_room.emit_state(now, "hostConnected", "host socket connected");
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(RoomJoin {
            player_index,
            resume_token: resume_token.expose().to_string(),
            room,
        })
    }

    /// Adds a guest socket to Player 2 and returns the resume token.
    pub(super) async fn connect_guest_impl(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<RoomJoin, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let resume_token = self.resume_token_generator.generate();
        let now = self.clock.now();
        let player_index = stored_room.room.join_guest_with_resume(
            guest,
            connection_id,
            resume_token.hash(),
            now,
        )?;

        stored_room.emit_state(now, "guestConnected", "guest socket connected");
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(RoomJoin {
            player_index,
            resume_token: resume_token.expose().to_string(),
            room,
        })
    }

    /// Reclaims a player slot after transport loss.
    pub(super) async fn reconnect_player_impl(
        &self,
        invite_code: InviteCode,
        player_index: PlayerIndex,
        room_epoch: u64,
        resume_token: String,
        connection_id: ConnectionId,
    ) -> Result<RoomJoin, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        stored_room.room.reconnect_player(
            player_index,
            &hash_resume_token(&resume_token),
            room_epoch,
            connection_id,
            now,
        )?;
        stored_room.emit_state(now, "playerReconnected", "player reconnected");
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(RoomJoin {
            player_index,
            resume_token,
            room,
        })
    }

    /// Handles a socket disconnection and removes closed rooms.
    pub(super) async fn disconnect_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let normalized = invite_code.normalized().to_string();
        let stored_room = rooms.get_mut(&normalized).ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let closed = stored_room.room.disconnect_with_recovery(
            connection_id,
            now,
            self.recovery_config.reconnect_grace,
        )?;

        stored_room.emit_state(now, "socketDisconnected", "socket disconnected");
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        if closed {
            rooms.remove(&normalized);
        }

        Ok(room)
    }

    /// Removes rooms abandoned by waiting hosts or expired recovery windows.
    pub(super) async fn sweep_expired_rooms(&self, now: Instant, join_timeout: Duration) -> usize {
        let mut rooms = self.invite_codes.write().await;
        let mut lifecycle_events = Vec::new();

        for stored_room in rooms.values_mut() {
            if stored_room.mark_stale_connections(
                now,
                self.recovery_config.heartbeat_stale,
                self.recovery_config.heartbeat_disconnect,
            ) {
                stored_room.emit_state(now, "heartbeatStale", "heartbeat stale");
                lifecycle_events.extend(stored_room.debug_events(1));
            }

            if stored_room.recover_stale_connections(
                now,
                self.recovery_config.heartbeat_disconnect,
                self.recovery_config.reconnect_grace,
            ) {
                stored_room.emit_state(now, "heartbeatTimedOut", "heartbeat timeout");
                lifecycle_events.extend(stored_room.debug_events(1));
            }
        }

        let before_count = rooms.len();
        rooms.retain(|_, stored_room| {
            !stored_room.is_expired_waiting(now, join_timeout)
                && !stored_room.is_expired_recovery(now)
                && !stored_room.is_idle_disconnected(now, self.recovery_config.room_idle)
        });
        let removed_count = before_count.saturating_sub(rooms.len());
        drop(rooms);
        self.record_recent_events(lifecycle_events);

        removed_count
    }
}
