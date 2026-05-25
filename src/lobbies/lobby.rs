//! Lobby domain state machine.
//!
//! The lobby owns player slots and selected-game state. It does not know about
//! HTTP requests, WebSockets, or external file-relay calls.

use crate::auth::VerifiedLicense;
use crate::lobbies::{
    LobbyClientCapabilities, LobbyError, LobbyGameCandidate, LobbyGameSelectionView,
    LobbyPlayerRole, LobbyPlayerSlot, LobbyPlayerStatus, LobbyServerCapabilities, LobbyView,
};
use crate::rooms::{
    ConnectionId, InviteCode, PlayerIndex, ResumeTokenHash, RoomId, hash_resume_token,
};
use serde::Serialize;

/// Maximum supported lobby size while game sessions remain focused on MVP rooms.
pub const MAX_LOBBY_PLAYERS: u8 = 4;

/// Lobby lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyStatus {
    /// Lobby is open and waiting for game selection or players.
    Open,
    /// A game has been selected and clients can prepare to launch.
    GameSelected,
    /// A child game room is active.
    InGame,
    /// Lobby has been closed.
    Closed,
}

/// Mutable lobby state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lobby {
    lobby_id: RoomId,
    invite_code: InviteCode,
    event_seq: u64,
    lobby_epoch: u64,
    created_at_ms: u128,
    updated_at_ms: u128,
    status: LobbyStatus,
    players: Vec<LobbyPlayerSlot>,
    selected_game: Option<LobbyGameSelectionView>,
}

impl Lobby {
    /// Creates a new lobby with Player 1 occupied by the host.
    pub fn new(
        invite_code: InviteCode,
        host: &VerifiedLicense,
        host_connection: ConnectionId,
        host_display_name: Option<String>,
        host_capabilities: LobbyClientCapabilities,
        host_resume_token_hash: ResumeTokenHash,
        initial_game: Option<LobbyGameCandidate>,
        now_ms: u128,
    ) -> Self {
        let mut players = (0..MAX_LOBBY_PLAYERS)
            .filter_map(|index| PlayerIndex::new(index, MAX_LOBBY_PLAYERS))
            .map(LobbyPlayerSlot::empty)
            .collect::<Vec<_>>();
        players[0] = LobbyPlayerSlot::host(
            host,
            host_connection,
            host_display_name,
            host_capabilities,
            host_resume_token_hash,
            now_ms,
        );
        let selected_game =
            initial_game.map(|game| LobbyGameSelectionView::new(game, PlayerIndex::ONE, now_ms));
        let status = if selected_game.is_some() {
            LobbyStatus::GameSelected
        } else {
            LobbyStatus::Open
        };

        Self {
            lobby_id: RoomId::new(),
            invite_code,
            event_seq: 1,
            lobby_epoch: 1,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            status,
            players,
            selected_game,
        }
    }

    /// Adds a guest or refreshes the existing slot for the same subject.
    pub fn join_or_refresh_player(
        &mut self,
        license: &VerifiedLicense,
        connection_id: ConnectionId,
        display_name: Option<String>,
        capabilities: LobbyClientCapabilities,
        resume_token_hash: ResumeTokenHash,
        now_ms: u128,
    ) -> Result<PlayerIndex, LobbyError> {
        if self.status == LobbyStatus::Closed {
            return Err(LobbyError::LobbyClosed);
        }

        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.belongs_to(license))
        {
            let role = slot.role;
            let player_index = slot.player_index;
            slot.occupy(
                role,
                license,
                connection_id,
                display_name,
                capabilities,
                resume_token_hash,
                now_ms,
            );
            self.bump(now_ms);
            return Ok(player_index);
        }

        let Some(slot) = self.players.iter_mut().find(|slot| slot.is_empty()) else {
            return Err(LobbyError::LobbyFull);
        };
        let player_index = slot.player_index;
        slot.occupy(
            LobbyPlayerRole::Guest,
            license,
            connection_id,
            display_name,
            capabilities,
            resume_token_hash,
            now_ms,
        );
        self.bump(now_ms);

        Ok(player_index)
    }

    /// Reclaims a lobby slot with a valid resume token.
    pub fn reconnect_player(
        &mut self,
        player_index: PlayerIndex,
        lobby_epoch: u64,
        resume_token: &str,
        connection_id: ConnectionId,
        now_ms: u128,
    ) -> Result<PlayerIndex, LobbyError> {
        if self.lobby_epoch != lobby_epoch {
            return Err(LobbyError::StaleLobbyEpoch);
        }
        let slot = self
            .slot_mut(player_index)
            .ok_or(LobbyError::PlayerSlotUnavailable)?;
        if slot.resume_token_hash.as_deref() != Some(hash_resume_token(resume_token).as_str()) {
            return Err(LobbyError::ResumeTokenInvalid);
        }

        slot.connection_id = Some(connection_id);
        slot.status = LobbyPlayerStatus::Connected;
        slot.last_seen_at_ms = Some(now_ms);
        self.bump(now_ms);

        Ok(player_index)
    }

    /// Marks one lobby socket disconnected while preserving the slot.
    pub fn disconnect(&mut self, connection_id: ConnectionId, now_ms: u128) -> bool {
        let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.connection_id == Some(connection_id))
        else {
            return false;
        };

        slot.connection_id = None;
        slot.status = LobbyPlayerStatus::Reconnecting;
        slot.last_seen_at_ms = Some(now_ms);
        self.bump(now_ms);

        true
    }

    /// Selects or replaces the game proposal for this lobby.
    pub fn select_game(
        &mut self,
        connection_id: ConnectionId,
        game: LobbyGameCandidate,
        now_ms: u128,
    ) -> Result<LobbyGameSelectionView, LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        let slot = self
            .slot(player_index)
            .ok_or(LobbyError::UnknownConnection)?;
        if slot.role != LobbyPlayerRole::Host {
            return Err(LobbyError::HostOnly);
        }
        validate_game_candidate(&game)?;

        let proposal = LobbyGameSelectionView::new(game, player_index, now_ms);
        self.selected_game = Some(proposal.clone());
        self.status = LobbyStatus::GameSelected;
        self.bump(now_ms);

        Ok(proposal)
    }

    /// Returns the player index assigned to a lobby connection.
    pub fn player_index_for_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, LobbyError> {
        self.players
            .iter()
            .find(|slot| slot.connection_id == Some(connection_id))
            .map(|slot| slot.player_index)
            .ok_or(LobbyError::UnknownConnection)
    }

    /// Returns the immutable lobby view for API clients.
    pub fn view(&self, capabilities: LobbyServerCapabilities) -> LobbyView {
        LobbyView {
            lobby_id: self.lobby_id,
            event_seq: self.event_seq,
            lobby_epoch: self.lobby_epoch,
            invite_code: self.invite_code.display(),
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            status: self.status,
            capabilities,
            players: self.players.iter().map(LobbyPlayerSlot::view).collect(),
            selected_game: self.selected_game.clone(),
        }
    }

    fn bump(&mut self, now_ms: u128) {
        self.event_seq += 1;
        self.lobby_epoch += 1;
        self.updated_at_ms = now_ms;
    }

    fn slot(&self, player_index: PlayerIndex) -> Option<&LobbyPlayerSlot> {
        self.players
            .iter()
            .find(|slot| slot.player_index == player_index)
    }

    fn slot_mut(&mut self, player_index: PlayerIndex) -> Option<&mut LobbyPlayerSlot> {
        self.players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
    }
}

fn validate_game_candidate(game: &LobbyGameCandidate) -> Result<(), LobbyError> {
    if game.title.trim().is_empty()
        || game.system_id.trim().is_empty()
        || game.core_id.trim().is_empty()
    {
        return Err(LobbyError::InvalidPayload);
    }

    if let Some(hash) = game.content_sha256.as_ref()
        && (hash.len() != 64 || !hash.chars().all(|candidate| candidate.is_ascii_hexdigit()))
    {
        return Err(LobbyError::InvalidPayload);
    }

    Ok(())
}
