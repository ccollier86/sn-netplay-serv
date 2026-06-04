//! Direct-invite temporary ROM relay operations.

use crate::protocol::{
    RomRelayBlockReason, RomRelayCancelled, RomRelayCompletion, RomRelayFailure, RomRelayGrant,
    RomRelayGrantRole, RomRelayProgress,
};
use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerIndex, PlayerRole, RomRelayGrantPair, RomRelayTransferIntent,
    RomRelayTransferState,
};

impl NetplayRoom {
    /// Validates a guest request for temporary ROM relay.
    pub fn prepare_rom_relay_transfer(
        &self,
        connection_id: ConnectionId,
    ) -> Result<RomRelayTransferIntent, RomRelayBlockReason> {
        let receiver_player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RomRelayBlockReason::WrongPlayer)?;
        if receiver_player_index == PlayerIndex::ONE {
            return Err(RomRelayBlockReason::WrongPlayer);
        }
        if self.rom_relay_transfer.is_some() {
            return Err(RomRelayBlockReason::TransferActive);
        }
        let capability = self
            .session
            .rom_relay
            .as_ref()
            .ok_or(RomRelayBlockReason::Disabled)?;
        if !capability.available {
            return Err(capability_reason_to_block(capability.reason));
        }
        let rom = self
            .session
            .rom_identity
            .clone()
            .ok_or(RomRelayBlockReason::MissingIdentity)?;

        let sender = self
            .players
            .iter()
            .find(|slot| slot.role == PlayerRole::Host)
            .ok_or(RomRelayBlockReason::WrongPlayer)?;
        let receiver = self
            .players
            .iter()
            .find(|slot| slot.player_index == receiver_player_index)
            .ok_or(RomRelayBlockReason::WrongPlayer)?;
        if !sender.supports_rom_file_relay || !receiver.supports_rom_file_relay {
            return Err(RomRelayBlockReason::ClientUnsupported);
        }
        let sender_connection = sender
            .connection_id
            .ok_or(RomRelayBlockReason::WrongPlayer)?;
        let receiver_connection = receiver
            .connection_id
            .ok_or(RomRelayBlockReason::WrongPlayer)?;

        Ok(RomRelayTransferIntent {
            room_id: self.room_id(),
            sender_player_index: sender.player_index,
            sender_connection,
            receiver_player_index,
            receiver_connection,
            rom,
            room_epoch: self.room_epoch,
            session_epoch: self.session_epoch,
        })
    }

    /// Stores file-relay grants and returns the private upload grant.
    pub fn accept_rom_relay_grants(
        &mut self,
        request_connection: ConnectionId,
        grants: RomRelayGrantPair,
    ) -> Result<RomRelayGrant, RomRelayBlockReason> {
        let intent = self.prepare_rom_relay_transfer(request_connection)?;
        let upload = grants.upload.clone();
        self.rom_relay_transfer = Some(RomRelayTransferState::new(
            intent.sender_connection,
            intent.receiver_connection,
            intent.sender_player_index,
            intent.receiver_player_index,
            grants,
            intent.rom,
        ));

        Ok(upload)
    }

    /// Records upload/download completion and returns the peer grant when the
    /// upload is complete.
    pub fn accept_rom_relay_completion(
        &mut self,
        source: ConnectionId,
        completion: RomRelayCompletion,
    ) -> Result<Option<(ConnectionId, RomRelayGrant)>, RomRelayBlockReason> {
        let Some(transfer) = self.rom_relay_transfer.as_mut() else {
            return Err(RomRelayBlockReason::MissingIdentity);
        };
        let peer = transfer.accept_completion(source, &completion)?;
        if completion.role == RomRelayGrantRole::Upload {
            return Ok(Some((peer, transfer.download_grant.clone())));
        }
        if completion.role == RomRelayGrantRole::Download {
            self.rom_relay_transfer = None;
        }

        Ok(None)
    }

    /// Validates that a progress event belongs to the active transfer.
    pub fn accept_rom_relay_progress(
        &self,
        source: ConnectionId,
        progress: &RomRelayProgress,
    ) -> Result<(), RomRelayBlockReason> {
        let Some(transfer) = self.rom_relay_transfer.as_ref() else {
            return Err(RomRelayBlockReason::MissingIdentity);
        };
        if progress.transfer_id != transfer.upload_grant.transfer_id {
            return Err(RomRelayBlockReason::MissingIdentity);
        }
        match progress.role {
            RomRelayGrantRole::Upload if source == transfer.sender_connection => Ok(()),
            RomRelayGrantRole::Download if source == transfer.receiver_connection => Ok(()),
            _ => Err(RomRelayBlockReason::WrongPlayer),
        }
    }

    /// Validates and clears an active failed transfer.
    pub fn accept_rom_relay_failure(
        &mut self,
        source: ConnectionId,
        failure: &RomRelayFailure,
    ) -> Result<(), RomRelayBlockReason> {
        self.ensure_rom_relay_participant(source, failure.transfer_id.as_deref())?;
        self.rom_relay_transfer = None;
        Ok(())
    }

    /// Validates and clears an active cancelled transfer.
    pub fn accept_rom_relay_cancelled(
        &mut self,
        source: ConnectionId,
        cancelled: &RomRelayCancelled,
    ) -> Result<(), RomRelayBlockReason> {
        self.ensure_rom_relay_participant(source, cancelled.transfer_id.as_deref())?;
        self.rom_relay_transfer = None;
        Ok(())
    }

    fn ensure_rom_relay_participant(
        &self,
        source: ConnectionId,
        transfer_id: Option<&str>,
    ) -> Result<(), RomRelayBlockReason> {
        let Some(transfer) = self.rom_relay_transfer.as_ref() else {
            return Err(RomRelayBlockReason::MissingIdentity);
        };
        if transfer_id.is_some_and(|id| id != transfer.upload_grant.transfer_id) {
            return Err(RomRelayBlockReason::MissingIdentity);
        }
        if source == transfer.sender_connection || source == transfer.receiver_connection {
            return Ok(());
        }
        Err(RomRelayBlockReason::WrongPlayer)
    }
}

fn capability_reason_to_block(
    reason: Option<crate::protocol::RomRelayCapabilityReason>,
) -> RomRelayBlockReason {
    match reason {
        Some(crate::protocol::RomRelayCapabilityReason::Disabled) => RomRelayBlockReason::Disabled,
        Some(crate::protocol::RomRelayCapabilityReason::BrokerUnavailable) => {
            RomRelayBlockReason::BrokerUnavailable
        }
        Some(crate::protocol::RomRelayCapabilityReason::MissingIdentity) => {
            RomRelayBlockReason::MissingIdentity
        }
        Some(crate::protocol::RomRelayCapabilityReason::TooLarge) => RomRelayBlockReason::TooLarge,
        Some(crate::protocol::RomRelayCapabilityReason::UnsupportedSystem) => {
            RomRelayBlockReason::UnsupportedSystem
        }
        Some(crate::protocol::RomRelayCapabilityReason::UnsupportedRoom) | None => {
            RomRelayBlockReason::Disabled
        }
    }
}
