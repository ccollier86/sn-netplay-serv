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
    ClientTransportCapabilities, ConnectionId, InviteCode, NetplayRoom, PlayerIndex,
    PlayerReconnectRequest, RoomError, RoomJoin, RoomView, hash_resume_token,
};
use std::time::{Duration, Instant};

impl InMemoryRoomRegistry {
    /// Creates a room and reserves Player 1 for the host.
    pub(super) async fn create_room_impl(
        &self,
        host: VerifiedLicense,
        host_connection: ConnectionId,
        session: NetplaySessionDescriptor,
        protocol_version: u16,
    ) -> Result<RoomView, RoomError> {
        let invite_code = self.invite_code_generator.generate();
        let resume_token = self.resume_token_generator.generate();
        let input_socket_token = self.resume_token_generator.generate();
        let now = self.clock.now();
        let mut room = NetplayRoom::new_with_protocol_and_resume(
            host,
            host_connection,
            invite_code.clone(),
            session,
            protocol_version,
            resume_token.hash(),
            input_socket_token.hash(),
            now,
        );
        if let Some(voice) = self.create_voice_state_for_room(&room).await {
            room.set_voice_state(voice);
        }
        let view = room.view_for_event(0, now);

        self.invite_codes.write().await.insert(
            invite_code.normalized().to_string(),
            StoredRoom::new(room, now),
        );

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
        let input_socket_token = self.resume_token_generator.generate();
        let now = self.clock.now();
        let player_index = stored_room.room.join_guest_with_resume(
            guest,
            connection_id,
            resume_token.hash(),
            input_socket_token.hash(),
            now,
            ClientTransportCapabilities::default(),
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
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let resume_token = self.resume_token_generator.generate();
        let input_socket_token = self.resume_token_generator.generate();
        let now = self.clock.now();
        let player_index = stored_room.room.attach_host_with_resume(
            host,
            connection_id,
            resume_token.hash(),
            input_socket_token.hash(),
            now,
            capabilities,
        )?;

        stored_room.emit_state(now, "hostConnected", "host socket connected");
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(RoomJoin {
            input_socket_token: input_socket_token.expose().to_string(),
            player_index,
            resume_token: resume_token.expose().to_string(),
            voice: stored_room.room.voice_grant_for(player_index),
            room,
        })
    }

    /// Adds a guest socket to Player 2 and returns the resume token.
    pub(super) async fn connect_guest_impl(
        &self,
        invite_code: InviteCode,
        guest: VerifiedLicense,
        connection_id: ConnectionId,
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let resume_token = self.resume_token_generator.generate();
        let input_socket_token = self.resume_token_generator.generate();
        let now = self.clock.now();
        let player_index = stored_room.room.join_guest_with_resume(
            guest,
            connection_id,
            resume_token.hash(),
            input_socket_token.hash(),
            now,
            capabilities,
        )?;

        stored_room.emit_state(now, "guestConnected", "guest socket connected");
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(RoomJoin {
            input_socket_token: input_socket_token.expose().to_string(),
            player_index,
            resume_token: resume_token.expose().to_string(),
            voice: stored_room.room.voice_grant_for(player_index),
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
        capabilities: ClientTransportCapabilities,
    ) -> Result<RoomJoin, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let next_resume_token = self.resume_token_generator.generate();
        let input_socket_token = self.resume_token_generator.generate();
        let resume_token_hash = hash_resume_token(&resume_token);
        stored_room.room.reconnect_player(PlayerReconnectRequest {
            player_index,
            resume_token_hash: &resume_token_hash,
            next_resume_token_hash: next_resume_token.hash(),
            input_socket_token_hash: input_socket_token.hash(),
            room_epoch,
            connection_id,
            now,
            capabilities,
        })?;
        stored_room.emit_state(now, "playerReconnected", "player reconnected");
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(RoomJoin {
            input_socket_token: input_socket_token.expose().to_string(),
            player_index,
            resume_token: next_resume_token.expose().to_string(),
            voice: stored_room.room.voice_grant_for(player_index),
            room,
        })
    }

    /// Arms an initial join before capability delivery for a runner takeover.
    pub(super) async fn arm_runner_handoff_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();

        stored_room.room.arm_runner_handoff(
            connection_id,
            now,
            self.recovery_config.runner_handoff_grace,
        )?;
        stored_room.record_debug_event(
            now,
            "runnerHandoffArmed",
            "initial control slot armed for runner handoff",
        );
        self.record_recent_events(stored_room.debug_events(1));

        Ok(())
    }

    /// Cancels a handoff when its `RoomJoined` capability was not delivered.
    pub(super) async fn cancel_runner_handoff_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room.room.cancel_runner_handoff(connection_id)
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
        let detail = stored_room
            .room
            .describe_control_connection(connection_id, "room cleanup");
        let closed = stored_room.room.disconnect_with_recovery(
            connection_id,
            now,
            self.recovery_config.reconnect_grace,
        )?;

        stored_room.emit_state(now, "socketDisconnected", &detail);
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        let voice_cleanup = if closed {
            stored_room.room.voice_room_id_for_cleanup()
        } else {
            None
        };

        if closed {
            rooms.remove(&normalized);
        }
        drop(rooms);
        self.cleanup_voice_room(voice_cleanup, "netplay-room-closed");

        Ok(room)
    }

    /// Records transport close diagnostics before room lifecycle cleanup runs.
    pub(super) async fn record_transport_close_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        socket_kind: &'static str,
        reason: String,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let detail = match socket_kind {
            "input" => stored_room
                .room
                .describe_input_connection(connection_id, &reason),
            _ => stored_room
                .room
                .describe_control_connection(connection_id, &reason),
        };
        let kind = match socket_kind {
            "input" => "inputSocketTransportClosed",
            _ => "socketTransportClosed",
        };

        stored_room.record_debug_event(now, kind, &detail);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(())
    }

    /// Ends a room because one player intentionally left.
    pub(super) async fn player_exited_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        reason: String,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let player_index = stored_room.room.player_exited(connection_id, now)?;

        stored_room.emit_player_exited(
            now,
            player_index.zero_based(),
            normalize_exit_reason(reason),
        );
        let voice_cleanup = stored_room.room.take_voice_room_id_for_cleanup();
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));
        drop(rooms);
        self.cleanup_voice_room(voice_cleanup, "player-exited");

        Ok(room)
    }

    /// Attaches a binary input socket to an occupied player slot.
    pub(super) async fn connect_input_socket_impl(
        &self,
        invite_code: InviteCode,
        player_index: PlayerIndex,
        room_epoch: u64,
        session_epoch: u64,
        input_socket_token: String,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();

        stored_room.room.attach_input_socket(
            player_index,
            room_epoch,
            session_epoch,
            &hash_resume_token(&input_socket_token),
            connection_id,
            now,
        )?;
        stored_room.emit_state(now, "inputSocketConnected", "input socket connected");
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(room)
    }

    /// Detaches a binary input socket from a room.
    pub(super) async fn disconnect_input_socket_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let detail = stored_room
            .room
            .describe_input_connection(connection_id, "room cleanup");

        stored_room.room.disconnect_input_socket(
            connection_id,
            now,
            self.recovery_config.reconnect_grace,
        )?;
        stored_room.emit_state(now, "inputSocketDisconnected", &detail);
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(room)
    }

    /// Removes rooms abandoned by waiting hosts or expired recovery windows.
    pub(super) async fn sweep_expired_rooms(&self, now: Instant, join_timeout: Duration) -> usize {
        let mut rooms = self.invite_codes.write().await;
        let mut lifecycle_events = Vec::new();
        let mut handoff_closed_keys = Vec::new();
        let mut state_recovery_closed_keys = Vec::new();

        for (key, stored_room) in rooms.iter_mut() {
            if stored_room.expire_state_recovery(now) {
                lifecycle_events.extend(stored_room.debug_events(1));
                state_recovery_closed_keys.push(key.clone());
                continue;
            }

            if let Some(room_closed) = stored_room.room.expire_runner_handoffs(now) {
                stored_room.emit_state(
                    now,
                    "runnerHandoffExpired",
                    "runner handoff deadline expired",
                );
                lifecycle_events.extend(stored_room.debug_events(1));
                if room_closed {
                    handoff_closed_keys.push(key.clone());
                }
            }

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

        let mut expired_keys = rooms
            .iter()
            .filter_map(|(key, stored_room)| {
                let expired = stored_room.is_expired_waiting(now, join_timeout)
                    || stored_room.is_expired_recovery(now)
                    || stored_room.is_idle_disconnected(now, self.recovery_config.room_idle);
                expired.then_some(key.clone())
            })
            .collect::<Vec<_>>();
        expired_keys.extend(handoff_closed_keys);
        expired_keys.extend(state_recovery_closed_keys);
        expired_keys.sort();
        expired_keys.dedup();
        let mut voice_cleanup = Vec::new();
        for key in &expired_keys {
            if let Some(mut stored_room) = rooms.remove(key) {
                voice_cleanup.push(stored_room.room.take_voice_room_id_for_cleanup());
            }
        }
        let removed_count = expired_keys.len();
        drop(rooms);
        self.record_recent_events(lifecycle_events);
        for voice_room_id in voice_cleanup {
            self.cleanup_voice_room(voice_room_id, "room-expired");
        }

        removed_count
    }
}

fn normalize_exit_reason(reason: String) -> String {
    let reason = reason.trim();

    if reason.is_empty() {
        return "userQuit".to_string();
    }

    reason.chars().take(80).collect()
}
