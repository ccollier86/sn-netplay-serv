//! Snapshot synchronization behavior for active rooms.
//!
//! This module owns host snapshot validation and file-relay transfer state so
//! the main room model stays focused on lifecycle and slot coordination.

use crate::protocol::{
    NetplaySessionMode, SnapshotChunk, SnapshotFileRelayGrant, SnapshotFileRelayGrantPair,
    SnapshotLimits, SnapshotManifest,
};
use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerRole, RoomError, RoomStatus, SnapshotFileRelayTransferIntent,
    SnapshotFileRelayTransferState, SnapshotTransferState,
};

impl NetplayRoom {
    /// Validates host snapshot chunk relay.
    pub fn accept_snapshot_chunk(
        &mut self,
        connection_id: ConnectionId,
        chunk: &SnapshotChunk,
        limits: SnapshotLimits,
    ) -> Result<(), RoomError> {
        self.validate_host_snapshot_sender(connection_id)?;
        self.validate_snapshot_repair_frame(chunk.repair_frame)?;
        self.validate_v5_recovery_snapshot_chunk(chunk)?;
        if self.host_snapshot_completed {
            return Err(RoomError::SnapshotInvalid);
        }
        self.snapshot_transfer
            .get_or_insert_with(SnapshotTransferState::new)
            .accept_chunk(chunk, limits)
    }

    /// Validates host snapshot completion metadata.
    pub fn accept_snapshot_complete(
        &mut self,
        connection_id: ConnectionId,
        manifest: &SnapshotManifest,
        limits: SnapshotLimits,
    ) -> Result<(), RoomError> {
        self.validate_host_snapshot_sender(connection_id)?;
        self.validate_snapshot_repair_frame(manifest.repair_frame)?;
        self.validate_v5_recovery_snapshot(manifest)?;
        let transfer = self
            .snapshot_transfer
            .as_ref()
            .ok_or(RoomError::SnapshotInvalid)?;
        transfer.complete(manifest, limits)?;
        self.snapshot_transfer = None;
        self.host_snapshot_completed = true;

        Ok(())
    }

    /// Validates that the host may request a file-relay snapshot transfer.
    pub fn prepare_snapshot_file_relay_transfer(
        &self,
        connection_id: ConnectionId,
        manifest: &SnapshotManifest,
        limits: SnapshotLimits,
    ) -> Result<SnapshotFileRelayTransferIntent, RoomError> {
        self.validate_host_snapshot_sender(connection_id)?;
        self.validate_snapshot_repair_frame(manifest.repair_frame)?;
        self.validate_v5_recovery_snapshot(manifest)?;
        manifest
            .validate(limits)
            .map_err(|_| RoomError::SnapshotInvalid)?;

        if self.host_snapshot_completed
            || self.snapshot_transfer.is_some()
            || self.snapshot_file_relay_transfer.is_some()
        {
            return Err(RoomError::SnapshotInvalid);
        }

        if !self.connected_players_support_state_file_relay() {
            return Err(RoomError::SnapshotFileRelayUnavailable);
        }

        let sender_player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let receiver = self
            .players
            .iter()
            .find(|slot| slot.connection_id.is_some() && slot.player_index != sender_player_index)
            .ok_or(RoomError::RoomNotReady)?;
        let receiver_connection = receiver.connection_id.ok_or(RoomError::RoomNotReady)?;

        Ok(SnapshotFileRelayTransferIntent {
            room_id: self.room_id(),
            sender_player_index,
            receiver_player_index: receiver.player_index,
            receiver_connection,
        })
    }

    /// Stores a file-relay snapshot transfer and returns the host upload grant.
    pub fn accept_snapshot_file_relay_grant(
        &mut self,
        connection_id: ConnectionId,
        manifest: &SnapshotManifest,
        grant_pair: SnapshotFileRelayGrantPair,
        limits: SnapshotLimits,
    ) -> Result<SnapshotFileRelayGrant, RoomError> {
        let intent = self.prepare_snapshot_file_relay_transfer(connection_id, manifest, limits)?;

        if grant_pair.upload.manifest != *manifest
            || grant_pair.download.manifest != *manifest
            || grant_pair.upload.transfer_id != grant_pair.download.transfer_id
        {
            return Err(RoomError::SnapshotInvalid);
        }

        self.snapshot_file_relay_transfer = Some(SnapshotFileRelayTransferState::new(
            connection_id,
            intent.receiver_player_index,
            grant_pair.download,
            manifest.clone(),
        ));

        Ok(grant_pair.upload)
    }

    /// Marks a file-relay snapshot upload complete and returns guest download
    /// routing.
    pub fn accept_snapshot_file_relay_upload_complete(
        &mut self,
        connection_id: ConnectionId,
        transfer_id: &str,
        manifest: &SnapshotManifest,
        limits: SnapshotLimits,
    ) -> Result<(ConnectionId, SnapshotFileRelayGrant), RoomError> {
        self.validate_host_snapshot_sender(connection_id)?;
        self.validate_snapshot_repair_frame(manifest.repair_frame)?;
        self.validate_v5_recovery_snapshot(manifest)?;
        manifest
            .validate(limits)
            .map_err(|_| RoomError::SnapshotInvalid)?;

        let transfer = self
            .snapshot_file_relay_transfer
            .take()
            .ok_or(RoomError::SnapshotInvalid)?;

        if transfer.source_connection != connection_id
            || transfer.download_grant.transfer_id != transfer_id
            || transfer.manifest != *manifest
        {
            self.snapshot_file_relay_transfer = Some(transfer);
            return Err(RoomError::SnapshotInvalid);
        }

        let receiver_connection = self
            .players
            .iter()
            .find(|slot| slot.player_index == transfer.receiver_player_index)
            .and_then(|slot| slot.connection_id)
            .ok_or(RoomError::RoomNotReady)?;

        self.host_snapshot_completed = true;

        Ok((receiver_connection, transfer.download_grant))
    }

    fn role_for_connection(&self, connection_id: ConnectionId) -> Option<PlayerRole> {
        self.players
            .iter()
            .find(|slot| slot.connection_id == Some(connection_id))
            .map(|slot| slot.role)
    }

    fn validate_host_snapshot_sender(&self, connection_id: ConnectionId) -> Result<(), RoomError> {
        if self.session.mode != NetplaySessionMode::ControllerNetplay {
            return Err(RoomError::RoomNotReady);
        }

        if self.status != RoomStatus::SyncingState && self.status != RoomStatus::Ready {
            return Err(RoomError::RoomNotReady);
        }

        match self.role_for_connection(connection_id) {
            Some(PlayerRole::Host) => Ok(()),
            Some(PlayerRole::Guest) => Err(RoomError::HostOnly),
            None => Err(RoomError::UnknownConnection),
        }
    }

    fn validate_snapshot_repair_frame(&self, repair_frame: u64) -> Result<(), RoomError> {
        if repair_frame == self.sync_start_frame {
            Ok(())
        } else {
            Err(RoomError::SnapshotInvalid)
        }
    }

    fn connected_players_support_state_file_relay(&self) -> bool {
        let connected_players = self.connected_player_indices();

        connected_players.len() == usize::from(self.max_players)
            && connected_players.iter().all(|player_index| {
                self.players
                    .iter()
                    .find(|slot| slot.player_index == *player_index)
                    .is_some_and(|slot| slot.supports_state_file_relay)
            })
    }
}
