//! Room-owned state for direct-invite temporary ROM relay transfers.

use crate::protocol::{
    RomIdentity, RomRelayBlockReason, RomRelayCompletion, RomRelayGrant, RomRelayGrantRole,
    normalize_content_hash,
};
use crate::rooms::{ConnectionId, PlayerIndex, RoomId};

/// Validated room data needed before calling the trusted file-relay service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomRelayTransferIntent {
    /// Room id that owns this temporary transfer.
    pub room_id: RoomId,
    /// Player that will upload the ROM.
    pub sender_player_index: PlayerIndex,
    /// Sender active room socket.
    pub sender_connection: ConnectionId,
    /// Player that will download the ROM.
    pub receiver_player_index: PlayerIndex,
    /// Receiver active room socket.
    pub receiver_connection: ConnectionId,
    /// ROM identity scoped to the transfer.
    pub rom: RomIdentity,
    /// Current room epoch.
    pub room_epoch: u64,
    /// Current session epoch.
    pub session_epoch: u64,
}

/// Pair of upload/download grants returned by the file relay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomRelayGrantPair {
    /// Host upload grant.
    pub upload: RomRelayGrant,
    /// Guest download grant.
    pub download: RomRelayGrant,
}

/// Active ROM relay transfer waiting for upload/download completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomRelayTransferState {
    /// Host connection that owns the upload.
    pub sender_connection: ConnectionId,
    /// Guest connection that owns the download.
    pub receiver_connection: ConnectionId,
    /// Sender player slot.
    pub sender_player_index: PlayerIndex,
    /// Receiver player slot.
    pub receiver_player_index: PlayerIndex,
    /// Upload grant.
    pub upload_grant: RomRelayGrant,
    /// Download grant held until upload completion.
    pub download_grant: RomRelayGrant,
    /// ROM identity the transfer must satisfy.
    pub rom: RomIdentity,
    /// Whether upload has been acknowledged by the sender.
    pub upload_complete: bool,
    /// Whether download has been verified by the receiver.
    pub download_complete: bool,
}

impl RomRelayTransferState {
    /// Creates a pending ROM transfer record.
    pub fn new(
        sender_connection: ConnectionId,
        receiver_connection: ConnectionId,
        sender_player_index: PlayerIndex,
        receiver_player_index: PlayerIndex,
        grants: RomRelayGrantPair,
        rom: RomIdentity,
    ) -> Self {
        Self {
            sender_connection,
            receiver_connection,
            sender_player_index,
            receiver_player_index,
            upload_grant: grants.upload,
            download_grant: grants.download,
            rom,
            upload_complete: false,
            download_complete: false,
        }
    }

    /// Validates a completion acknowledgement and returns the peer connection
    /// that should receive the resulting event/grant.
    pub fn accept_completion(
        &mut self,
        source: ConnectionId,
        completion: &RomRelayCompletion,
    ) -> Result<ConnectionId, RomRelayBlockReason> {
        let expected_hash = self.rom.normalized_hash();
        if normalize_content_hash(&completion.content_hash) != expected_hash {
            return Err(RomRelayBlockReason::MissingIdentity);
        }
        if completion.transfer_id != self.upload_grant.transfer_id {
            return Err(RomRelayBlockReason::MissingIdentity);
        }

        match completion.role {
            RomRelayGrantRole::Upload if source == self.sender_connection => {
                self.upload_complete = true;
                Ok(self.receiver_connection)
            }
            RomRelayGrantRole::Download if source == self.receiver_connection => {
                if !self.upload_complete {
                    return Err(RomRelayBlockReason::TransferActive);
                }
                self.download_complete = true;
                Ok(self.sender_connection)
            }
            _ => Err(RomRelayBlockReason::WrongPlayer),
        }
    }
}
