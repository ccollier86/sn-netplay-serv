//! Temporary ROM relay preparation for lobbies.
//!
//! The lobby validates who may request a temporary transfer. The file relay
//! service still owns byte movement and token issuance.

use crate::lobbies::{Lobby, LobbyError, LobbyGameCandidate, LobbyPlayerRole};
use crate::rooms::{ConnectionId, PlayerIndex, RoomId};
use uuid::Uuid;

/// Runtime limits for lobby ROM relay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LobbyRomRelayLimits {
    /// Maximum ROM bytes the relay may prepare for one session.
    pub max_bytes: u64,
}

/// Validated temporary ROM relay intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyRomRelayTransferIntent {
    /// Lobby id that owns this transfer.
    pub lobby_id: RoomId,
    /// Selected game proposal this transfer belongs to.
    pub proposal_id: Uuid,
    /// Proposed game being prepared.
    pub game: LobbyGameCandidate,
    /// Verified complete payload SHA-256.
    pub sha256: String,
    /// Verified complete payload byte size.
    pub size_bytes: u64,
    /// Sender player slot.
    pub sender_player_index: PlayerIndex,
    /// Sender active lobby socket.
    pub sender_connection_id: ConnectionId,
    /// Receiver player slot.
    pub receiver_player_index: PlayerIndex,
    /// Receiver active lobby socket.
    pub receiver_connection_id: ConnectionId,
}

impl Lobby {
    /// Validates that the host can prepare a temporary ROM transfer.
    pub fn prepare_rom_relay_transfer(
        &self,
        connection_id: ConnectionId,
        proposal_id: Uuid,
        receiver_player_index: PlayerIndex,
        limits: LobbyRomRelayLimits,
    ) -> Result<LobbyRomRelayTransferIntent, LobbyError> {
        let sender_player_index = self.player_index_for_connection(connection_id)?;
        let sender = self
            .slot(sender_player_index)
            .ok_or(LobbyError::UnknownConnection)?;
        if sender.role != LobbyPlayerRole::Host {
            return Err(LobbyError::HostOnly);
        }

        let receiver = self
            .slot(receiver_player_index)
            .ok_or(LobbyError::PlayerSlotUnavailable)?;
        let receiver_connection_id = receiver
            .connection_id
            .ok_or(LobbyError::PlayerSlotUnavailable)?;
        if !sender.capabilities.supports_temporary_session_rom_relay
            || !receiver.capabilities.supports_temporary_session_rom_relay
        {
            return Err(LobbyError::RomRelayUnsupported);
        }

        let selected_game = self
            .selected_game()
            .filter(|candidate| candidate.proposal_id == proposal_id)
            .ok_or(LobbyError::StaleGameProposal)?;
        let sha256 = selected_game
            .game
            .content_sha256
            .as_ref()
            .ok_or(LobbyError::InvalidPayload)?;
        let size_bytes = selected_game
            .game
            .rom_size_bytes
            .ok_or(LobbyError::InvalidPayload)?;
        if sha256.len() != 64
            || !sha256
                .chars()
                .all(|candidate| candidate.is_ascii_hexdigit())
        {
            return Err(LobbyError::InvalidPayload);
        }
        if size_bytes > limits.max_bytes {
            return Err(LobbyError::RomRelayTooLarge);
        }

        Ok(LobbyRomRelayTransferIntent {
            lobby_id: self.lobby_id(),
            proposal_id,
            game: selected_game.game.clone(),
            sha256: sha256.clone(),
            size_bytes,
            sender_player_index,
            sender_connection_id: connection_id,
            receiver_player_index,
            receiver_connection_id,
        })
    }

    /// Revalidates a transfer intent immediately before private grants emit.
    pub fn require_rom_relay_transfer_current(
        &self,
        intent: &LobbyRomRelayTransferIntent,
    ) -> Result<(), LobbyError> {
        self.require_selected_proposal(intent.proposal_id)?;

        let sender = self
            .slot(intent.sender_player_index)
            .ok_or(LobbyError::UnknownConnection)?;
        let receiver = self
            .slot(intent.receiver_player_index)
            .ok_or(LobbyError::PlayerSlotUnavailable)?;
        if sender.connection_id != Some(intent.sender_connection_id)
            || receiver.connection_id != Some(intent.receiver_connection_id)
        {
            return Err(LobbyError::PlayerSlotUnavailable);
        }

        Ok(())
    }
}
