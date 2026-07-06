//! Selected startup-state relay preparation for lobbies.
//!
//! The lobby validates who may request a startup-state transfer and that the
//! request still matches the selected proposal. File relay token creation and
//! byte movement stay outside the lobby domain.

use crate::lobbies::{Lobby, LobbyError, LobbyGameCandidate, LobbyPlayerRole};
use crate::protocol::LobbyStartupStateTransferMetadata;
use crate::rooms::{ConnectionId, PlayerIndex, RoomId};
use uuid::Uuid;

/// Runtime limits for lobby startup-state relay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LobbyStartupStateRelayLimits {
    /// Maximum startup-state bytes the relay may prepare for one session.
    pub max_bytes: u64,
}

/// Validated selected startup-state relay intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyStartupStateRelayTransferIntent {
    /// Lobby id that owns this transfer.
    pub lobby_id: RoomId,
    /// Selected game proposal this transfer belongs to.
    pub proposal_id: Uuid,
    /// Proposed game being prepared.
    pub game: LobbyGameCandidate,
    /// Sender-provided startup-state metadata after lobby validation.
    pub state: LobbyStartupStateTransferMetadata,
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
    /// Validates that the host can prepare a selected startup-state transfer.
    pub fn prepare_startup_state_relay_transfer(
        &self,
        connection_id: ConnectionId,
        proposal_id: Uuid,
        receiver_player_index: PlayerIndex,
        mut state: LobbyStartupStateTransferMetadata,
        limits: LobbyStartupStateRelayLimits,
    ) -> Result<LobbyStartupStateRelayTransferIntent, LobbyError> {
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
            return Err(LobbyError::StartupStateRelayUnsupported);
        }

        let selected_game = self
            .selected_game()
            .filter(|candidate| candidate.proposal_id == proposal_id)
            .ok_or(LobbyError::StaleGameProposal)?;
        let selected_label = selected_game
            .game
            .start_state_label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .ok_or(LobbyError::InvalidPayload)?;
        if !valid_startup_state_metadata(&state, selected_label) {
            return Err(LobbyError::InvalidPayload);
        }
        if state.size_bytes > limits.max_bytes {
            return Err(LobbyError::StartupStateRelayTooLarge);
        }
        if state
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .is_none()
        {
            state.label = Some(selected_label.to_owned());
        }

        Ok(LobbyStartupStateRelayTransferIntent {
            lobby_id: self.lobby_id(),
            proposal_id,
            game: selected_game.game.clone(),
            state,
            sender_player_index,
            sender_connection_id: connection_id,
            receiver_player_index,
            receiver_connection_id,
        })
    }

    /// Revalidates a startup-state transfer intent before private grants emit.
    pub fn require_startup_state_relay_transfer_current(
        &self,
        intent: &LobbyStartupStateRelayTransferIntent,
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

fn valid_startup_state_metadata(
    state: &LobbyStartupStateTransferMetadata,
    selected_label: &str,
) -> bool {
    let valid_hash = state.sha256.len() == 64
        && state
            .sha256
            .chars()
            .all(|candidate| candidate.is_ascii_hexdigit());
    let label_matches = state
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .is_none_or(|label| label == selected_label);
    let state_format_valid = state
        .state_format
        .as_deref()
        .is_none_or(|value| !value.trim().is_empty());
    valid_hash && state.size_bytes > 0 && label_matches && state_format_valid
}
