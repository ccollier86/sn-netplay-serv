//! Lobby domain state machine.
//!
//! The lobby owns player slots and selected-game state. It does not know about
//! HTTP requests, WebSockets, or external file-relay calls.

use crate::auth::VerifiedLicense;
use crate::lobbies::{
    LOBBY_LINK_CABLE_CONTRACT_VERSION, LobbyActivityKind, LobbyClientCapabilities, LobbyError,
    LobbyGameCandidate, LobbyGameLaunchView, LobbyGameReadinessStatus, LobbyGameReadinessView,
    LobbyGameSelectionView, LobbyLinkCableClientCapabilities, LobbyLinkCableLaunchState,
    LobbyLinkCablePlayerSlotView, LobbyLinkCableView, LobbyLinkProtocolFamily,
    LobbyMultiplayerExtension, LobbyMultiplayerSessionKind, LobbyPlayerOccupancy,
    LobbyPlayerRemoval, LobbyPlayerRole, LobbyPlayerSlot, LobbyPlayerStatus, LobbyReturnOutcome,
    LobbyReturnRequest, LobbyReturnedView, LobbyServerCapabilities, LobbyView,
    MAX_LINK_CABLE_LOBBY_PLAYERS,
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
    multiplayer_extension: Option<LobbyMultiplayerExtension>,
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
            multiplayer_extension: None,
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
        if self.multiplayer_extension.is_some() {
            self.require_link_capability(&capabilities)?;
            let is_existing_player = self.players.iter().any(|slot| slot.belongs_to(license));
            if !is_existing_player && self.occupied_player_count() >= MAX_LINK_CABLE_LOBBY_PLAYERS {
                return Err(LobbyError::LobbyFull);
            }
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
        if self.multiplayer_extension.is_some() {
            self.require_link_capability(&request.capabilities)?;
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
        self.clear_link_player(player_index, now_ms);
        self.pending_launch = None;
        self.status = if self.selected_game.is_some() {
            LobbyStatus::GameSelected
        } else {
            LobbyStatus::Open
        };
        self.bump_with_activity(now_ms);

        Ok(())
    }

    /// Permanently removes an occupied guest slot on behalf of the host.
    pub(crate) fn remove_player(
        &mut self,
        requester_connection_id: ConnectionId,
        lobby_epoch: u64,
        target_player_index: PlayerIndex,
        now_ms: u128,
    ) -> Result<LobbyPlayerRemoval, LobbyError> {
        if self.lobby_epoch != lobby_epoch {
            return Err(LobbyError::StaleLobbyEpoch);
        }
        let requester_index = self.player_index_for_connection(requester_connection_id)?;
        let requester = self
            .slot(requester_index)
            .ok_or(LobbyError::UnknownConnection)?;
        if requester.role != LobbyPlayerRole::Host {
            return Err(LobbyError::PlayerRemovalHostOnly);
        }
        if self.status == LobbyStatus::Closed
            || self.status == LobbyStatus::InGame
            || self.pending_launch.is_some()
        {
            return Err(LobbyError::LobbyPlayerRemovalUnavailable);
        }

        let target = self
            .slot(target_player_index)
            .ok_or(LobbyError::LobbyPlayerNotFound)?;
        if target.role == LobbyPlayerRole::Host {
            return Err(LobbyError::CannotRemoveLobbyHost);
        }
        if target.subject_key.is_none() {
            return Err(LobbyError::LobbyPlayerNotFound);
        }
        let connection_id = target.connection_id;
        let voice = self.voice_grant_for(target_player_index);

        if let Some(slot) = self.slot_mut(target_player_index) {
            *slot = LobbyPlayerSlot::empty(target_player_index);
        }
        self.game_readiness
            .retain(|readiness| readiness.player_index != target_player_index.zero_based());
        self.clear_link_player(target_player_index, now_ms);
        self.status = if self.selected_game.is_some() {
            LobbyStatus::GameSelected
        } else {
            LobbyStatus::Open
        };
        self.bump_with_activity(now_ms);

        Ok(LobbyPlayerRemoval {
            player_index: target_player_index,
            connection_id,
            voice_room_id: voice.as_ref().map(|grant| grant.voice_room_id.clone()),
            participant_identity: voice.map(|grant| grant.participant_identity),
        })
    }

    /// Selects or replaces the game proposal for this lobby.
    pub fn select_game(
        &mut self,
        connection_id: ConnectionId,
        game: LobbyGameCandidate,
        now_ms: u128,
    ) -> Result<LobbyGameSelectionView, LobbyError> {
        if self.multiplayer_extension.is_some() {
            return Err(LobbyError::ControllerOperationUnavailableInLinkMode);
        }
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

    /// Selects or replaces one player's local game in a link-cable lobby.
    pub fn select_link_cable_game(
        &mut self,
        connection_id: ConnectionId,
        game: LobbyGameCandidate,
        protocol_family: LobbyLinkProtocolFamily,
        room_invite_code: Option<InviteCode>,
        now_ms: u128,
    ) -> Result<LobbyLinkCablePlayerSlotView, LobbyError> {
        validate_game_candidate(&game)?;
        if !game_supports_link_protocol_family(&game, protocol_family) {
            return Err(LobbyError::LinkCableFamilyMismatch);
        }
        let player_index = self.player_index_for_connection(connection_id)?;
        if player_index.zero_based() >= MAX_LINK_CABLE_LOBBY_PLAYERS {
            return Err(LobbyError::LobbyFull);
        }
        let (player_role, player_capabilities) = self
            .slot(player_index)
            .map(|player| (player.role, player.capabilities.clone()))
            .ok_or(LobbyError::UnknownConnection)?;
        let occupied_player_indexes = self
            .players
            .iter()
            .filter(|player| player.subject_key.is_some())
            .map(|player| player.player_index.zero_based())
            .collect::<Vec<_>>();
        if self.multiplayer_extension.is_some() {
            self.require_link_capability(&player_capabilities)?;
        } else {
            require_standalone_link_capability(&player_capabilities, protocol_family)?;
        }

        let mut extension = match self.multiplayer_extension.clone() {
            Some(extension) => {
                let link = extension
                    .link_cable
                    .as_ref()
                    .ok_or(LobbyError::ControllerOperationUnavailableInLinkMode)?;
                if extension.session_kind != LobbyMultiplayerSessionKind::LinkCable
                    || link.protocol_family != protocol_family
                {
                    return Err(LobbyError::LinkCableFamilyMismatch);
                }
                extension
            }
            None => {
                if player_role != LobbyPlayerRole::Host {
                    return Err(LobbyError::HostOnly);
                }
                if self.occupied_player_count() > MAX_LINK_CABLE_LOBBY_PLAYERS {
                    return Err(LobbyError::LobbyFull);
                }
                self.require_all_occupied_players_support_link(protocol_family)?;
                let room_invite_code = room_invite_code
                    .as_ref()
                    .ok_or(LobbyError::InvalidPayload)?
                    .display();
                LobbyMultiplayerExtension {
                    session_kind: LobbyMultiplayerSessionKind::LinkCable,
                    generation: 1,
                    link_cable: Some(LobbyLinkCableView {
                        protocol_family,
                        max_players: MAX_LINK_CABLE_LOBBY_PLAYERS,
                        room_invite_code: Some(room_invite_code),
                        cable_epoch: None,
                        players: initial_link_player_slots(now_ms),
                    }),
                }
            }
        };

        let link = extension
            .link_cable
            .as_mut()
            .ok_or(LobbyError::ControllerOperationUnavailableInLinkMode)?;
        if let Some(room_invite_code) = room_invite_code {
            let normalized_invite = room_invite_code.display();
            if player_role == LobbyPlayerRole::Host {
                if link.room_invite_code.as_deref() != Some(normalized_invite.as_str()) {
                    extension.generation = extension
                        .generation
                        .checked_add(1)
                        .ok_or(LobbyError::InvalidPayload)?;
                    link.room_invite_code = Some(normalized_invite);
                    link.cable_epoch = None;
                    for slot in &mut link.players {
                        if occupied_player_indexes.contains(&slot.player_index) {
                            slot.selection_generation = slot
                                .selection_generation
                                .checked_add(1)
                                .ok_or(LobbyError::InvalidPayload)?;
                        }
                        slot.launch_state = LobbyLinkCableLaunchState::NotLaunched;
                        slot.updated_at_ms = now_ms;
                    }
                }
            } else if link.room_invite_code.as_deref() != Some(normalized_invite.as_str()) {
                return Err(LobbyError::InvalidPayload);
            }
        }

        let selected_slot = link
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index.zero_based())
            .ok_or(LobbyError::PlayerSlotUnavailable)?;
        selected_slot.selection_generation = selected_slot
            .selection_generation
            .checked_add(1)
            .ok_or(LobbyError::InvalidPayload)?;
        selected_slot.selected_game = Some(game.clone());
        selected_slot.launch_state = LobbyLinkCableLaunchState::NotLaunched;
        selected_slot.updated_at_ms = now_ms;
        let selected_slot = selected_slot.clone();

        if player_role == LobbyPlayerRole::Host {
            self.selected_game = Some(LobbyGameSelectionView::new(game, player_index, now_ms));
        }
        self.multiplayer_extension = Some(extension);
        self.game_readiness.clear();
        self.pending_launch = None;
        self.last_return = None;
        self.status = LobbyStatus::GameSelected;
        self.bump_with_activity(now_ms);

        Ok(selected_slot)
    }

    /// Updates one player's independent link-cable launch/runtime state.
    pub fn set_link_cable_launch_state(
        &mut self,
        connection_id: ConnectionId,
        selection_generation: u64,
        state: LobbyLinkCableLaunchState,
        room_invite_code: Option<InviteCode>,
        now_ms: u128,
    ) -> Result<LobbyLinkCablePlayerSlotView, LobbyError> {
        let player_index = self.player_index_for_connection(connection_id)?;
        self.slot(player_index)
            .ok_or(LobbyError::UnknownConnection)?;
        let extension = self
            .multiplayer_extension
            .as_mut()
            .ok_or(LobbyError::ControllerOperationUnavailableInLinkMode)?;
        if extension.session_kind != LobbyMultiplayerSessionKind::LinkCable {
            return Err(LobbyError::ControllerOperationUnavailableInLinkMode);
        }
        let link = extension
            .link_cable
            .as_mut()
            .ok_or(LobbyError::ControllerOperationUnavailableInLinkMode)?;
        if let Some(room_invite_code) = room_invite_code {
            let normalized_invite = room_invite_code.display();
            if link.room_invite_code.as_deref() != Some(normalized_invite.as_str()) {
                return Err(LobbyError::InvalidPayload);
            }
        }
        if link.room_invite_code.is_none() {
            return Err(LobbyError::GameLaunchNotReady);
        }
        let selected_slot = link
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index.zero_based())
            .ok_or(LobbyError::PlayerSlotUnavailable)?;
        if selected_slot.selection_generation != selection_generation {
            return Err(LobbyError::StaleLinkCableSelection);
        }
        if selected_slot.selected_game.is_none() {
            return Err(LobbyError::GameLaunchNotReady);
        }
        selected_slot.launch_state = state;
        selected_slot.updated_at_ms = now_ms;
        let selected_slot = selected_slot.clone();
        self.bump_with_activity(now_ms);

        Ok(selected_slot)
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
        if self.multiplayer_extension.is_some() {
            return Err(LobbyError::ControllerOperationUnavailableInLinkMode);
        }
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
        if self.multiplayer_extension.is_some() {
            return Err(LobbyError::ControllerOperationUnavailableInLinkMode);
        }
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
        if self.multiplayer_extension.is_some() {
            return Err(LobbyError::ControllerOperationUnavailableInLinkMode);
        }
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
        if self.multiplayer_extension.is_some() {
            return Err(LobbyError::ControllerOperationUnavailableInLinkMode);
        }
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
        if self.multiplayer_extension.is_some() {
            return Err(LobbyError::ControllerOperationUnavailableInLinkMode);
        }
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

    /// Returns whether this lobby resolved to per-player link-cable play.
    pub(super) fn is_link_cable_mode(&self) -> bool {
        self.multiplayer_extension
            .as_ref()
            .is_some_and(|extension| {
                extension.session_kind == LobbyMultiplayerSessionKind::LinkCable
                    && extension.link_cable.is_some()
            })
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
    pub fn view(&self, mut capabilities: LobbyServerCapabilities) -> LobbyView {
        if self.multiplayer_extension.is_some() {
            capabilities.max_players = MAX_LINK_CABLE_LOBBY_PLAYERS;
        }
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
            multiplayer_extension: self.multiplayer_extension.clone(),
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

    fn occupied_player_count(&self) -> u8 {
        self.players
            .iter()
            .filter(|slot| slot.subject_key.is_some())
            .count()
            .try_into()
            .unwrap_or(MAX_LOBBY_PLAYERS)
    }

    fn require_link_capability(
        &self,
        candidate: &LobbyClientCapabilities,
    ) -> Result<(), LobbyError> {
        let link = self
            .multiplayer_extension
            .as_ref()
            .and_then(|extension| extension.link_cable.as_ref())
            .ok_or(LobbyError::ControllerOperationUnavailableInLinkMode)?;
        let host_capability = self
            .players
            .first()
            .and_then(|slot| slot.capabilities.link_cable.as_ref())
            .ok_or(LobbyError::LinkCableCapabilityRequired)?;
        require_matching_link_capability(candidate, link.protocol_family, host_capability)
    }

    fn require_all_occupied_players_support_link(
        &self,
        protocol_family: LobbyLinkProtocolFamily,
    ) -> Result<(), LobbyError> {
        let host_capability = self
            .players
            .first()
            .and_then(|slot| slot.capabilities.link_cable.as_ref())
            .ok_or(LobbyError::LinkCableCapabilityRequired)?;
        require_standalone_link_capability(&self.players[0].capabilities, protocol_family)?;
        for player in self
            .players
            .iter()
            .filter(|slot| slot.subject_key.is_some())
        {
            require_matching_link_capability(
                &player.capabilities,
                protocol_family,
                host_capability,
            )?;
        }
        Ok(())
    }

    fn clear_link_player(&mut self, player_index: PlayerIndex, now_ms: u128) {
        let occupied_player_indexes = self
            .players
            .iter()
            .filter(|player| player.subject_key.is_some())
            .map(|player| player.player_index.zero_based())
            .collect::<Vec<_>>();
        let Some(extension) = self.multiplayer_extension.as_mut() else {
            return;
        };
        let Some(link) = extension.link_cable.as_mut() else {
            return;
        };

        extension.generation = extension.generation.saturating_add(1);
        link.room_invite_code = None;
        link.cable_epoch = None;
        for slot in &mut link.players {
            if slot.player_index == player_index.zero_based() {
                slot.selection_generation = slot.selection_generation.saturating_add(1);
                slot.selected_game = None;
                slot.launch_state = LobbyLinkCableLaunchState::Stopped;
                slot.updated_at_ms = now_ms;
            } else if occupied_player_indexes.contains(&slot.player_index) {
                slot.selection_generation = slot.selection_generation.saturating_add(1);
                slot.launch_state = if slot.selected_game.is_some() {
                    LobbyLinkCableLaunchState::Interrupted
                } else {
                    LobbyLinkCableLaunchState::NotLaunched
                };
                slot.updated_at_ms = now_ms;
            }
        }
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

fn initial_link_player_slots(now_ms: u128) -> Vec<LobbyLinkCablePlayerSlotView> {
    (0..MAX_LINK_CABLE_LOBBY_PLAYERS)
        .map(|player_index| LobbyLinkCablePlayerSlotView {
            player_index,
            cable_slot: player_index,
            selection_generation: 0,
            selected_game: None,
            launch_state: LobbyLinkCableLaunchState::NotLaunched,
            updated_at_ms: now_ms,
        })
        .collect()
}

fn game_supports_link_protocol_family(
    game: &LobbyGameCandidate,
    protocol_family: LobbyLinkProtocolFamily,
) -> bool {
    if !game.core_id.trim().eq_ignore_ascii_case("mgba") {
        return false;
    }
    match (
        game.system_id.trim().to_ascii_lowercase().as_str(),
        protocol_family,
    ) {
        ("gb" | "gbc", LobbyLinkProtocolFamily::GbSerialV1)
        | ("gba", LobbyLinkProtocolFamily::GbaMultiV1 | LobbyLinkProtocolFamily::GbaMultiV2) => {
            true
        }
        _ => false,
    }
}

fn require_standalone_link_capability(
    candidate: &LobbyClientCapabilities,
    protocol_family: LobbyLinkProtocolFamily,
) -> Result<(), LobbyError> {
    let link = candidate
        .link_cable
        .as_ref()
        .ok_or(LobbyError::LinkCableCapabilityRequired)?;
    if link.contract_version != LOBBY_LINK_CABLE_CONTRACT_VERSION
        || link.runtime_profile.trim().is_empty()
        || link.core_build_id.trim().is_empty()
        || !link.protocol_families.contains(&protocol_family)
    {
        return Err(LobbyError::LinkCableCapabilityRequired);
    }
    Ok(())
}

fn require_matching_link_capability(
    candidate: &LobbyClientCapabilities,
    protocol_family: LobbyLinkProtocolFamily,
    required: &LobbyLinkCableClientCapabilities,
) -> Result<(), LobbyError> {
    require_standalone_link_capability(candidate, protocol_family)?;
    let candidate = candidate
        .link_cable
        .as_ref()
        .ok_or(LobbyError::LinkCableCapabilityRequired)?;
    if required.contract_version != LOBBY_LINK_CABLE_CONTRACT_VERSION
        || !required.protocol_families.contains(&protocol_family)
        || candidate.contract_version != required.contract_version
        || candidate.runtime_profile != required.runtime_profile
        || candidate.core_build_id != required.core_build_id
    {
        return Err(LobbyError::LinkCableCapabilityRequired);
    }
    Ok(())
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
