//! Registry operations for direct-invite temporary ROM relay.

use super::InMemoryRoomRegistry;
use crate::protocol::{
    RomRelayBlockReason, RomRelayCancelled, RomRelayCompletion, RomRelayFailure, RomRelayProgress,
};
use crate::rooms::stored_room::StoredRoom;
use crate::rooms::{ConnectionId, InviteCode, RomRelayGrantPair, RomRelayTransferIntent};

impl InMemoryRoomRegistry {
    /// Validates a guest request for direct-invite ROM relay.
    pub(super) async fn prepare_rom_relay_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<RomRelayTransferIntent, RomRelayBlockReason> {
        let rooms = self.invite_codes.read().await;
        let stored_room = rooms
            .get(invite_code.normalized())
            .ok_or(RomRelayBlockReason::MissingIdentity)?;

        stored_room.room.prepare_rom_relay_transfer(connection_id)
    }

    /// Stores ROM relay grants and privately sends host upload.
    pub(super) async fn grant_rom_relay_upload_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        grants: RomRelayGrantPair,
    ) -> Result<(), RomRelayBlockReason> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RomRelayBlockReason::MissingIdentity)?;
        let upload_grant = stored_room
            .room
            .accept_rom_relay_grants(connection_id, grants)?;
        let now = self.clock.now();
        stored_room.emit_rom_relay_upload_granted(now, upload_grant);
        self.record_recent_events(stored_room.debug_events(1));

        Ok(())
    }

    /// Emits ROM relay progress.
    pub(super) async fn relay_rom_relay_progress_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        progress: RomRelayProgress,
    ) -> Result<(), RomRelayBlockReason> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = self.rom_relay_room_mut(&mut rooms, &invite_code)?;
        stored_room
            .room
            .accept_rom_relay_progress(connection_id, &progress)?;
        let now = self.clock.now();
        stored_room.emit_rom_relay_progress(now, connection_id, progress);
        self.record_recent_events(stored_room.debug_events(1));
        Ok(())
    }

    /// Emits ROM relay completion and grants guest download after upload.
    pub(super) async fn relay_rom_relay_completed_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        completion: RomRelayCompletion,
    ) -> Result<(), RomRelayBlockReason> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = self.rom_relay_room_mut(&mut rooms, &invite_code)?;
        let download = stored_room
            .room
            .accept_rom_relay_completion(connection_id, completion.clone())?;
        let now = self.clock.now();
        stored_room.emit_rom_relay_completed(now, connection_id, completion);
        if let Some((_receiver, grant)) = download {
            stored_room.emit_rom_relay_download_granted(now, grant);
        }
        self.record_recent_events(stored_room.debug_events(1));
        Ok(())
    }

    /// Emits ROM relay failure and clears active transfer.
    pub(super) async fn relay_rom_relay_failed_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        failure: RomRelayFailure,
    ) -> Result<(), RomRelayBlockReason> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = self.rom_relay_room_mut(&mut rooms, &invite_code)?;
        stored_room
            .room
            .accept_rom_relay_failure(connection_id, &failure)?;
        let now = self.clock.now();
        stored_room.emit_rom_relay_failed(now, connection_id, failure);
        self.record_recent_events(stored_room.debug_events(1));
        Ok(())
    }

    /// Emits ROM relay cancellation and clears active transfer.
    pub(super) async fn relay_rom_relay_cancelled_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        cancelled: RomRelayCancelled,
    ) -> Result<(), RomRelayBlockReason> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = self.rom_relay_room_mut(&mut rooms, &invite_code)?;
        stored_room
            .room
            .accept_rom_relay_cancelled(connection_id, &cancelled)?;
        let now = self.clock.now();
        stored_room.emit_rom_relay_cancelled(now, connection_id, cancelled);
        self.record_recent_events(stored_room.debug_events(1));
        Ok(())
    }

    fn rom_relay_room_mut<'a>(
        &self,
        rooms: &'a mut std::collections::HashMap<String, StoredRoom>,
        invite_code: &InviteCode,
    ) -> Result<&'a mut StoredRoom, RomRelayBlockReason> {
        rooms
            .get_mut(invite_code.normalized())
            .ok_or(RomRelayBlockReason::MissingIdentity)
    }
}
