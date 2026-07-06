//! Lobby domain state machine.
//!
//! The lobby owns player slots and selected-game state. It does not know about
//! HTTP requests, WebSockets, or external file-relay calls.

use crate::auth::VerifiedLicense;
use crate::lobbies::{
    LobbyActivityKind, LobbyClientCapabilities, LobbyError, LobbyGameCandidate,
    LobbyGameLaunchView, LobbyGameReadinessStatus, LobbyGameReadinessView, LobbyGameSelectionView,
    LobbyPlayerOccupancy, LobbyPlayerRole, LobbyPlayerSlot, LobbyPlayerStatus, LobbyReturnOutcome,
    LobbyReturnRequest, LobbyReturnedView, LobbyServerCapabilities, LobbyView,
};
use crate::rooms::{
    ConnectionId, InviteCode, PlayerIndex, ResumeTokenHash, RoomId, RoomVoiceState,
    hash_resume_token,
};
use serde::{Deserialize, Serialize};

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

/// Whether a lobby can appear in public discovery.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyVisibility {
    /// Lobby can only be joined through its invite code.
    #[default]
    Private,
    /// Lobby can be listed for other signed-in desktop clients.
    Public,
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
    last_meaningful_activity_at_ms: u128,
    status: LobbyStatus,
    visibility: LobbyVisibility,
    players: Vec<LobbyPlayerSlot>,
    selected_game: Option<LobbyGameSelectionView>,
    game_readiness: Vec<LobbyGameReadinessView>,
    pending_launch: Option<LobbyGameLaunchView>,
    last_return: Option<LobbyReturnedView>,
    pub(crate) voice: Option<RoomVoiceState>,
}

/// Data required to create a lobby and reserve the host slot.
pub struct LobbyCreateRequest<'a> {
    /// Invite code assigned to the lobby.
    pub invite_code: InviteCode,
    /// Verified host identity.
    pub host: &'a VerifiedLicense,
    /// Active host lobby connection.
    pub host_connection: ConnectionId,
    /// Optional host display name.
    pub host_display_name: Option<String>,
    /// Host client feature support.
    pub host_capabilities: LobbyClientCapabilities,
    /// One-way hash of the host resume token.
    pub host_resume_token_hash: ResumeTokenHash,
    /// Optional first game selected for the lobby.
    pub initial_game: Option<LobbyGameCandidate>,
    /// Lobby discovery visibility.
    pub visibility: LobbyVisibility,
    /// Creation timestamp in milliseconds since unix epoch.
    pub now_ms: u128,
}

/// Data required to reclaim an existing lobby player slot.
pub struct LobbyReconnectRequest<'a> {
    /// Verified reconnecting player identity.
    pub license: &'a VerifiedLicense,
    /// Slot the player is trying to reclaim.
    pub player_index: PlayerIndex,
    /// Lobby epoch observed by the reconnecting client.
    pub lobby_epoch: u64,
    /// Raw resume token supplied by the reconnecting client.
    pub resume_token: &'a str,
    /// Fresh lobby control connection id.
    pub connection_id: ConnectionId,
    /// Optional display name refresh.
    pub display_name: Option<String>,
    /// Client feature support after reconnect.
    pub capabilities: LobbyClientCapabilities,
    /// Activity timestamp in milliseconds since unix epoch.
    pub now_ms: u128,
}

impl Lobby {
    /// Creates a new lobby with Player 1 occupied by the host.
    pub fn new(request: LobbyCreateRequest<'_>) -> Self {
        let mut players = (0..MAX_LOBBY_PLAYERS)
            .filter_map(|index| PlayerIndex::new(index, MAX_LOBBY_PLAYERS))
            .map(LobbyPlayerSlot::empty)
            .collect::<Vec<_>>();
        players[0] = LobbyPlayerSlot::host(
            request.host,
            request.host_connection,
            request.host_display_name,
            request.host_capabilities,
            request.host_resume_token_hash,
            request.now_ms,
        );
        let selected_game = request
            .initial_game
            .map(|game| LobbyGameSelectionView::new(game, PlayerIndex::ONE, request.now_ms));
        let status = if selected_game.is_some() {
            LobbyStatus::GameSelected
        } else {
            LobbyStatus::Open
        };

        Self {
            lobby_id: RoomId::new(),
            invite_code: request.invite_code,
            event_seq: 1,
            lobby_epoch: 1,
            created_at_ms: request.now_ms,
            updated_at_ms: request.now_ms,
            last_meaningful_activity_at_ms: request.now_ms,
            status,
            visibility: request.visibility,
            players,
            selected_game,
            game_readiness: Vec::new(),
            pending_launch: None,
            last_return: None,
            voice: None,
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
            slot.occupy(LobbyPlayerOccupancy {
                role,
                license,
                connection_id,
                display_name,
                capabilities,
                resume_token_hash,
                now_ms,
            });
            self.bump_with_activity(now_ms);
            return Ok(player_index);
        }

        let Some(slot) = self.players.iter_mut().find(|slot| slot.is_empty()) else {
            return Err(LobbyError::LobbyFull);
        };
        let player_index = slot.player_index;
        slot.occupy(LobbyPlayerOccupancy {
            role: LobbyPlayerRole::Guest,
            license,
            connection_id,
            display_name,
            capabilities,
            resume_token_hash,
            now_ms,
        });
        self.bump_with_activity(now_ms);

        Ok(player_index)
    }

    /// Reclaims a lobby slot with a valid resume token.
    pub fn reconnect_player(
        &mut self,
        request: LobbyReconnectRequest<'_>,
    ) -> Result<PlayerIndex, LobbyError> {
        if self.status == LobbyStatus::Closed {
            return Err(LobbyError::LobbyClosed);
        }
        if request.lobby_epoch > self.lobby_epoch {
            return Err(LobbyError::StaleLobbyEpoch);
        }
        let resume_token_hash = hash_resume_token(request.resume_token);
        let slot = self
            .slot_mut(request.player_index)
            .ok_or(LobbyError::PlayerSlotUnavailable)?;
        if !slot.belongs_to(request.license) {
            return Err(LobbyError::PlayerSlotUnavailable);
        }
        if slot.resume_token_hash.as_deref() != Some(resume_token_hash.as_str()) {
            return Err(LobbyError::ResumeTokenInvalid);
        }

        slot.occupy(LobbyPlayerOccupancy {
            role: slot.role,
            license: request.license,
            connection_id: request.connection_id,
            display_name: request.display_name,
            capabilities: request.capabilities,
            resume_token_hash,
            now_ms: request.now_ms,
        });
        self.bump_with_activity(request.now_ms);

        Ok(request.player_index)
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

    /// Ends lobby membership for a socket that intentionally leaves.
    pub fn leave(&mut self, connection_id: ConnectionId, now_ms: u128) -> Result<(), LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        let role = self
            .slot(player_index)
            .ok_or(LobbyError::UnknownConnection)?
            .role;

        if role == LobbyPlayerRole::Host {
            self.close(now_ms);
            return Ok(());
        }

        if let Some(slot) = self.slot_mut(player_index) {
            *slot = LobbyPlayerSlot::empty(player_index);
        }
        self.game_readiness
            .retain(|readiness| readiness.player_index != player_index.zero_based());
        self.pending_launch = None;
        self.status = if self.selected_game.is_some() {
            LobbyStatus::GameSelected
        } else {
            LobbyStatus::Open
        };
        self.bump_with_activity(now_ms);

        Ok(())
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
        self.game_readiness.clear();
        self.pending_launch = None;
        self.last_return = None;
        self.status = LobbyStatus::GameSelected;
        self.bump_with_activity(now_ms);

        Ok(proposal)
    }

    /// Records this player's readiness for the selected game proposal.
    pub fn set_game_readiness(
        &mut self,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        status: LobbyGameReadinessStatus,
        detail: Option<String>,
        now_ms: u128,
    ) -> Result<LobbyGameReadinessView, LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        self.require_selected_proposal(proposal_id)?;
        let readiness =
            LobbyGameReadinessView::new(player_index, proposal_id, status, detail, now_ms)?;

        if let Some(existing) = self
            .game_readiness
            .iter_mut()
            .find(|candidate| candidate.player_index == player_index.zero_based())
        {
            *existing = readiness.clone();
        } else {
            self.game_readiness.push(readiness.clone());
        }
        self.pending_launch = None;
        self.status = LobbyStatus::GameSelected;
        self.bump_with_activity(now_ms);

        Ok(readiness)
    }

    /// Creates a launch signal once every connected player is ready.
    pub fn request_game_launch(
        &mut self,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        now_ms: u128,
    ) -> Result<LobbyGameLaunchView, LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        let slot = self
            .slot(player_index)
            .ok_or(LobbyError::UnknownConnection)?;
        if slot.role != LobbyPlayerRole::Host {
            return Err(LobbyError::HostOnly);
        }
        self.require_selected_proposal(proposal_id)?;
        if !self.connected_players_are_ready(proposal_id) {
            return Err(LobbyError::PlayersNotReady);
        }

        let launch = LobbyGameLaunchView::new(proposal_id, player_index, now_ms);
        self.pending_launch = Some(launch.clone());
        self.last_return = None;
        self.status = LobbyStatus::InGame;
        self.bump_with_activity(now_ms);

        Ok(launch)
    }

    /// Publishes the gameplay room invite created by the host.
    pub fn publish_game_room(
        &mut self,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        room_invite_code: InviteCode,
        now_ms: u128,
    ) -> Result<LobbyGameLaunchView, LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        let slot = self
            .slot(player_index)
            .ok_or(LobbyError::UnknownConnection)?;
        if slot.role != LobbyPlayerRole::Host {
            return Err(LobbyError::HostOnly);
        }
        self.require_selected_proposal(proposal_id)?;
        let launch = self
            .pending_launch
            .as_mut()
            .ok_or(LobbyError::StaleGameProposal)?;
        if launch.proposal_id != proposal_id {
            return Err(LobbyError::StaleGameProposal);
        }

        launch.publish_room(room_invite_code.display(), now_ms);
        let launch = launch.clone();
        self.status = LobbyStatus::InGame;
        self.bump_with_activity(now_ms);

        Ok(launch)
    }

    /// Marks the active launch as playing after a runner reaches gameplay.
    pub fn mark_gameplay_started(
        &mut self,
        connection_id: ConnectionId,
        lobby_epoch: u64,
        proposal_id: uuid::Uuid,
        now_ms: u128,
    ) -> Result<bool, LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        let Some(slot) = self.slot(player_index) else {
            return Err(LobbyError::UnknownConnection);
        };
        if !slot.capabilities.supports_lobby_gameplay_started {
            return Ok(false);
        }
        self.require_selected_proposal(proposal_id)?;
        if lobby_epoch != self.lobby_epoch {
            return self
                .pending_launch
                .as_ref()
                .filter(|launch| {
                    launch.proposal_id == proposal_id
                        && launch.status == crate::lobbies::LobbyGameLaunchStatus::Playing
                })
                .map(|_| false)
                .ok_or(LobbyError::StaleLobbyEpoch);
        }
        let Some(expected_player_indexes) = self.gameplay_start_expected_player_indexes() else {
            return Ok(false);
        };
        let launch = self
            .pending_launch
            .as_mut()
            .ok_or(LobbyError::StaleGameProposal)?;
        if launch.proposal_id != proposal_id {
            return Err(LobbyError::StaleGameProposal);
        }

        let changed = launch.mark_player_started(player_index, &expected_player_indexes, now_ms)?;
        if changed {
            self.status = LobbyStatus::InGame;
            self.bump_with_activity(now_ms);
        }

        Ok(changed)
    }

    /// Clears an active child game and returns players to lobby readiness.
    pub fn return_to_lobby(
        &mut self,
        request: LobbyReturnRequest,
    ) -> Result<LobbyReturnOutcome, LobbyError> {
        self.player_index_for_connection(request.connection_id)?;
        self.require_selected_proposal(request.proposal_id)?;
        if request.lobby_epoch != self.lobby_epoch {
            return self
                .idempotent_return_outcome(request.proposal_id)
                .ok_or(LobbyError::StaleLobbyEpoch);
        }
        match self.pending_launch.as_ref() {
            Some(launch) if launch.proposal_id == request.proposal_id => {}
            _ => {
                return self
                    .idempotent_return_outcome(request.proposal_id)
                    .ok_or(LobbyError::StaleGameProposal);
            }
        }

        let returned = LobbyReturnedView::new(
            request.proposal_id,
            request.return_requested_by_player_index,
            request.reason,
            request.now_ms,
        );
        self.game_readiness.clear();
        self.pending_launch = None;
        self.status = if self.selected_game.is_some() {
            LobbyStatus::GameSelected
        } else {
            LobbyStatus::Open
        };
        self.last_return = Some(returned.clone());
        self.bump_with_activity(request.now_ms);

        Ok(LobbyReturnOutcome::applied(returned))
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

    /// Returns the current lobby epoch.
    pub fn lobby_epoch(&self) -> u64 {
        self.lobby_epoch
    }

    /// Records transport-confirmed activity that should retain this lobby.
    pub fn record_activity(
        &mut self,
        connection_id: ConnectionId,
        _kind: LobbyActivityKind,
        now_ms: u128,
    ) -> Result<(), LobbyError> {
        self.player_index_for_connection(connection_id)?;
        self.mark_meaningful_activity(now_ms);

        Ok(())
    }

    /// Returns this lobby's stable id.
    pub(super) fn lobby_id(&self) -> RoomId {
        self.lobby_id
    }

    /// Returns this lobby's invite code.
    pub(super) fn invite_code(&self) -> &InviteCode {
        &self.invite_code
    }

    /// Returns this lobby's lifecycle status.
    pub(super) fn status(&self) -> LobbyStatus {
        self.status
    }

    /// Returns the timestamp used by idle cleanup.
    pub(super) fn last_meaningful_activity_at_ms(&self) -> u128 {
        self.last_meaningful_activity_at_ms
    }

    /// Returns whether this lobby exceeded the meaningful-activity idle window.
    pub(super) fn is_meaningfully_idle(
        &self,
        now_ms: u128,
        idle_timeout: std::time::Duration,
    ) -> bool {
        if self.status == LobbyStatus::Closed {
            return false;
        }

        now_ms.saturating_sub(self.last_meaningful_activity_at_ms) >= idle_timeout.as_millis()
    }

    /// Closes the lobby for idle cleanup.
    pub(super) fn close_due_to_idle(&mut self, now_ms: u128) -> bool {
        if self.status == LobbyStatus::Closed {
            return false;
        }

        self.close(now_ms);
        true
    }

    /// Returns the current selected game proposal, if any.
    pub(super) fn selected_game(&self) -> Option<&LobbyGameSelectionView> {
        self.selected_game.as_ref()
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
            last_meaningful_activity_at_ms: self.last_meaningful_activity_at_ms,
            status: self.status,
            visibility: self.visibility,
            capabilities,
            players: self.players.iter().map(LobbyPlayerSlot::view).collect(),
            selected_game: self.selected_game.clone(),
            game_readiness: self.game_readiness.clone(),
            pending_launch: self.pending_launch.clone(),
            voice: self.voice.as_ref().map(RoomVoiceState::view),
        }
    }

    fn bump(&mut self, now_ms: u128) {
        self.event_seq += 1;
        self.lobby_epoch += 1;
        self.updated_at_ms = now_ms;
    }

    fn bump_with_activity(&mut self, now_ms: u128) {
        self.bump(now_ms);
        self.mark_meaningful_activity(now_ms);
    }

    pub(super) fn mark_meaningful_activity(&mut self, now_ms: u128) {
        self.last_meaningful_activity_at_ms = now_ms;
    }

    fn close(&mut self, now_ms: u128) {
        self.status = LobbyStatus::Closed;
        self.pending_launch = None;
        for slot in &mut self.players {
            if slot.subject_key.is_some() {
                slot.connection_id = None;
                slot.status = LobbyPlayerStatus::Disconnected;
                slot.last_seen_at_ms = Some(now_ms);
            }
        }
        self.bump(now_ms);
    }

    pub(super) fn slot(&self, player_index: PlayerIndex) -> Option<&LobbyPlayerSlot> {
        self.players
            .iter()
            .find(|slot| slot.player_index == player_index)
    }

    fn slot_mut(&mut self, player_index: PlayerIndex) -> Option<&mut LobbyPlayerSlot> {
        self.players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
    }

    pub(super) fn require_selected_proposal(
        &self,
        proposal_id: uuid::Uuid,
    ) -> Result<(), LobbyError> {
        match self.selected_game.as_ref() {
            Some(selected_game) if selected_game.proposal_id == proposal_id => Ok(()),
            _ => Err(LobbyError::StaleGameProposal),
        }
    }

    fn connected_players_are_ready(&self, proposal_id: uuid::Uuid) -> bool {
        self.players
            .iter()
            .filter(|slot| slot.subject_key.is_some() && slot.connection_id.is_some())
            .all(|slot| {
                self.game_readiness.iter().any(|readiness| {
                    readiness.player_index == slot.player_index.zero_based()
                        && readiness.proposal_id == proposal_id
                        && readiness.status == LobbyGameReadinessStatus::Ready
                })
            })
    }

    fn gameplay_start_expected_player_indexes(&self) -> Option<Vec<PlayerIndex>> {
        let connected_players = self
            .players
            .iter()
            .filter(|slot| slot.subject_key.is_some() && slot.connection_id.is_some())
            .collect::<Vec<_>>();
        if connected_players
            .iter()
            .any(|slot| !slot.capabilities.supports_lobby_gameplay_started)
        {
            return None;
        }
        Some(
            connected_players
                .into_iter()
                .map(|slot| slot.player_index)
                .collect(),
        )
    }

    fn idempotent_return_outcome(&self, proposal_id: uuid::Uuid) -> Option<LobbyReturnOutcome> {
        let returned = self.last_return.as_ref()?;
        (returned.proposal_id == proposal_id)
            .then(|| LobbyReturnOutcome::already_applied(returned.clone()))
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
