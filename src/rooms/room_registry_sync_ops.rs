//! Compatibility and state-sync helpers for the in-memory room registry.
//!
//! The registry remains the synchronization boundary, while this module keeps
//! compatibility, ready/start, and snapshot relay behavior isolated.

use super::InMemoryRoomRegistry;
use crate::protocol::{
    ClientNetworkQualityReport, CompatibilityFingerprint, LinkCableCompatibility, SnapshotChunk,
    SnapshotFileRelayGrantPair, SnapshotLimits, SnapshotManifest,
};
use crate::rooms::stored_room::StoredRoom;
use crate::rooms::{
    ConnectionId, InviteCode, RoomError, RoomView, SnapshotFileRelayTransferIntent,
};

impl InMemoryRoomRegistry {
    /// Stores controller-netplay compatibility from one connection.
    pub(super) async fn set_compatibility_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        fingerprint: CompatibilityFingerprint,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        match stored_room
            .room
            .set_compatibility_for_connection(connection_id, fingerprint)
        {
            Ok(()) => self.emit_room_view(
                stored_room,
                "compatibilitySet",
                "compatibility fingerprint set",
            ),
            Err(RoomError::CompatibilityMismatch) => {
                let now = self.clock.now();
                stored_room.emit_state(now, "compatibilityMismatch", "compatibility mismatch");
                self.record_recent_events(stored_room.debug_events(1));
                Err(RoomError::CompatibilityMismatch)
            }
            Err(error) => Err(error),
        }
    }

    /// Stores link-cable compatibility from one connection.
    pub(super) async fn set_link_cable_compatibility_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        compatibility: LinkCableCompatibility,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        match stored_room
            .room
            .set_link_cable_compatibility_for_connection(connection_id, compatibility)
        {
            Ok(()) => self.emit_room_view(
                stored_room,
                "linkCompatibilitySet",
                "link compatibility set",
            ),
            Err(RoomError::CompatibilityMismatch) => {
                let now = self.clock.now();
                stored_room.emit_state(
                    now,
                    "linkCompatibilityMismatch",
                    "link compatibility mismatch",
                );
                self.record_recent_events(stored_room.debug_events(1));
                Err(RoomError::CompatibilityMismatch)
            }
            Err(error) => Err(error),
        }
    }

    /// Marks a player ready and emits session start when all players are ready.
    pub(super) async fn mark_ready_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        network: Option<ClientNetworkQualityReport>,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let started = stored_room.room.mark_ready(connection_id, network, now)?;

        if started {
            stored_room.emit_start(now, stored_room.room.sync_start_frame());
        } else if stored_room.room.should_request_clock_sync_sample() {
            let request = stored_room
                .room
                .request_clock_sync_sample(self.server_time_ms_at(now));
            stored_room.emit_clock_sync_sample_requested(now, request);
        } else {
            stored_room.emit_state(now, "playerReady", "player marked ready");
        }
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(room)
    }

    /// Validates and broadcasts a host snapshot chunk.
    pub(super) async fn relay_snapshot_chunk_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        chunk: SnapshotChunk,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room
            .room
            .accept_snapshot_chunk(connection_id, &chunk, SnapshotLimits::default())?;
        let now = self.clock.now();
        stored_room.emit_snapshot_chunk(now, connection_id, chunk);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(())
    }

    /// Validates and broadcasts a host snapshot completion manifest.
    pub(super) async fn relay_snapshot_complete_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room.room.accept_snapshot_complete(
            connection_id,
            &manifest,
            SnapshotLimits::default(),
        )?;
        let now = self.clock.now();
        stored_room.emit_snapshot_complete(now, connection_id, manifest);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(())
    }

    /// Validates a large host snapshot before a file-relay grant is created.
    pub(super) async fn prepare_snapshot_file_relay_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
    ) -> Result<SnapshotFileRelayTransferIntent, RoomError> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        stored_room.room.prepare_snapshot_file_relay_transfer(
            connection_id,
            &manifest,
            SnapshotLimits::default(),
        )
    }

    /// Stores a file-relay transfer and sends the host's upload grant.
    pub(super) async fn grant_snapshot_file_relay_upload_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        manifest: SnapshotManifest,
        grant_pair: SnapshotFileRelayGrantPair,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        let upload_grant = stored_room.room.accept_snapshot_file_relay_grant(
            connection_id,
            &manifest,
            grant_pair,
            SnapshotLimits::default(),
        )?;
        let now = self.clock.now();
        stored_room.emit_snapshot_file_relay_upload_granted(now, connection_id, upload_grant);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(())
    }

    /// Completes a file-relay upload and sends the guest's download grant.
    pub(super) async fn relay_snapshot_file_upload_complete_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        transfer_id: String,
        manifest: SnapshotManifest,
    ) -> Result<(), RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;

        let (receiver, download_grant) = stored_room
            .room
            .accept_snapshot_file_relay_upload_complete(
                connection_id,
                &transfer_id,
                &manifest,
                SnapshotLimits::default(),
            )?;
        let now = self.clock.now();
        stored_room.emit_snapshot_file_relay_download_ready(now, receiver, download_grant);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(())
    }

    fn emit_room_view(
        &self,
        stored_room: &mut StoredRoom,
        kind: &'static str,
        detail: &'static str,
    ) -> Result<RoomView, RoomError> {
        let now = self.clock.now();
        stored_room.emit_state(now, kind, detail);
        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));
        Ok(room)
    }
}
