//! Domain model for one active netplay room.
//!
//! This module owns slot assignment, room status transitions, compatibility
//! checks, and input-frame validation. It does not store rooms globally or
//! perform network IO.

use crate::auth::VerifiedLicense;
use crate::limits::MVP_ROOM_CAPACITY;
use crate::protocol::{
    CompatibilityFingerprint, InputFrame, InputFrameLimits, NetplayProtocolView,
    NetplaySessionDescriptor, NetplaySessionMode, SnapshotChunk, SnapshotLimits, SnapshotManifest,
};
use crate::rooms::{
    ConnectionId, InviteCode, LinkCableRoomState, PlayerIndex, PlayerRole, PlayerSlot,
    PlayerSlotView, PlayerStatus, RoomError, RoomId, RoomStatus, RoomView, SnapshotTransferState,
};
use std::collections::{HashMap, HashSet};

/// Active netplay room.
#[derive(Clone, Debug)]
pub struct NetplayRoom {
    room_id: RoomId,
    invite_code: InviteCode,
    pub(super) session: NetplaySessionDescriptor,
    pub(super) max_players: u8,
    pub(super) players: Vec<PlayerSlot>,
    pub(super) status: RoomStatus,
    compatibility: HashMap<PlayerIndex, CompatibilityFingerprint>,
    pub(super) ready_players: HashSet<PlayerIndex>,
    last_input_frames: HashMap<PlayerIndex, u64>,
    pub(super) link_cable_state: LinkCableRoomState,
    host_snapshot_completed: bool,
    snapshot_transfer: Option<SnapshotTransferState>,
    room_frame: u64,
}

impl NetplayRoom {
    /// Creates a room and reserves Player 1 for the verified host.
    pub fn new(
        host: VerifiedLicense,
        host_connection: ConnectionId,
        invite_code: InviteCode,
        session: NetplaySessionDescriptor,
    ) -> Self {
        let max_players = MVP_ROOM_CAPACITY;
        let mut players = Vec::with_capacity(usize::from(max_players));
        players.push(PlayerSlot::host(&host, host_connection));

        for raw_index in 1..max_players {
            let index = PlayerIndex::new(raw_index, max_players).expect("valid mvp player index");
            players.push(PlayerSlot::empty(index));
        }

        Self {
            room_id: RoomId::new(),
            invite_code,
            session,
            max_players,
            players,
            status: RoomStatus::WaitingForGuest,
            compatibility: HashMap::new(),
            ready_players: HashSet::new(),
            last_input_frames: HashMap::new(),
            link_cable_state: LinkCableRoomState::default(),
            host_snapshot_completed: false,
            snapshot_transfer: None,
            room_frame: 0,
        }
    }

    /// Returns the stable room id.
    pub fn room_id(&self) -> RoomId {
        self.room_id
    }

    /// Returns the invite code used for lookups.
    pub fn invite_code(&self) -> &InviteCode {
        &self.invite_code
    }

    /// Returns the current room lifecycle status.
    pub fn status(&self) -> RoomStatus {
        self.status
    }

    /// Adds a guest to the first empty slot and returns their player index.
    pub fn join_guest(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let player_index = {
            let slot = self
                .players
                .iter_mut()
                .find(|candidate| candidate.is_empty())
                .ok_or(RoomError::RoomFull)?;
            slot.occupy_guest(&license, connection_id);
            slot.player_index
        };
        self.reset_sync_state();
        self.status = RoomStatus::CheckingCompatibility;
        self.players
            .iter_mut()
            .filter(|slot| !slot.is_empty())
            .for_each(|slot| slot.status = PlayerStatus::Connected);

        Ok(player_index)
    }

    /// Attaches a socket connection to the reserved host slot.
    pub fn attach_host(
        &mut self,
        license: VerifiedLicense,
        connection_id: ConnectionId,
    ) -> Result<PlayerIndex, RoomError> {
        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let slot = self
            .players
            .iter_mut()
            .find(|candidate| candidate.role == PlayerRole::Host)
            .ok_or(RoomError::UnknownConnection)?;
        let subject_matches = slot
            .subject_key
            .as_deref()
            .is_some_and(|subject_key| subject_key == license.identity_key());

        if !subject_matches {
            return Err(RoomError::HostSubjectMismatch);
        }

        slot.connection_id = Some(connection_id);
        slot.status = PlayerStatus::Connected;
        self.ready_players.remove(&slot.player_index);

        Ok(slot.player_index)
    }

    /// Marks the connection as disconnected and returns whether the room closed.
    pub fn disconnect(&mut self, connection_id: ConnectionId) -> Result<bool, RoomError> {
        let slot = self
            .players
            .iter_mut()
            .find(|slot| slot.connection_id == Some(connection_id))
            .ok_or(RoomError::UnknownConnection)?;

        let player_index = slot.player_index;
        let is_host = slot.role == PlayerRole::Host;

        slot.connection_id = None;
        slot.status = if is_host {
            PlayerStatus::Disconnected
        } else {
            PlayerStatus::Empty
        };

        self.compatibility.remove(&player_index);
        self.ready_players.remove(&player_index);
        self.last_input_frames.remove(&player_index);
        self.link_cable_state.reset();
        self.snapshot_transfer = None;

        if is_host {
            self.status = RoomStatus::Closed;
            return Ok(true);
        }

        self.status = RoomStatus::WaitingForGuest;

        Ok(false)
    }

    /// Stores a compatibility fingerprint for the connected player.
    pub fn set_compatibility_for_connection(
        &mut self,
        connection_id: ConnectionId,
        mut fingerprint: CompatibilityFingerprint,
    ) -> Result<(), RoomError> {
        if self.session.mode != NetplaySessionMode::ControllerNetplay {
            return Err(RoomError::CompatibilityMismatch);
        }

        if self.status == RoomStatus::Closed {
            return Err(RoomError::RoomClosed);
        }

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        if !self.fingerprint_matches_session(&fingerprint) {
            self.compatibility.remove(&player_index);
            self.ready_players.remove(&player_index);
            self.snapshot_transfer = None;
            self.status = RoomStatus::CheckingCompatibility;
            self.set_player_status(player_index, PlayerStatus::CompatibilityFailed);
            return Err(RoomError::CompatibilityMismatch);
        }

        fingerprint.content_hash = fingerprint.content_hash.to_ascii_lowercase();
        self.compatibility.insert(player_index, fingerprint);
        self.ready_players.remove(&player_index);
        self.set_player_status(player_index, PlayerStatus::CheckingCompatibility);

        if !self.connected_players_have_fingerprints() {
            return Ok(());
        }

        let mut fingerprints = self.compatibility.values();
        let baseline = fingerprints.next().expect("at least one fingerprint");
        if fingerprints.any(|candidate| baseline.first_mismatch(candidate).is_some()) {
            self.status = RoomStatus::CheckingCompatibility;
            self.players
                .iter_mut()
                .filter(|slot| !slot.is_empty())
                .for_each(|slot| slot.status = PlayerStatus::CompatibilityFailed);
            return Err(RoomError::CompatibilityMismatch);
        }

        self.status = RoomStatus::SyncingState;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| slot.status = PlayerStatus::SyncingState);

        Ok(())
    }

    /// Marks a connected player ready and starts when every player is ready.
    pub fn mark_ready(&mut self, connection_id: ConnectionId) -> Result<bool, RoomError> {
        if self.status != RoomStatus::SyncingState && self.status != RoomStatus::Ready {
            return Err(RoomError::RoomNotReady);
        }

        if self.session.mode == NetplaySessionMode::ControllerNetplay
            && !self.host_snapshot_completed
        {
            return Err(RoomError::RoomNotReady);
        }

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        self.ready_players.insert(player_index);
        self.set_player_status(player_index, PlayerStatus::Ready);

        if !self.connected_players_are_ready() {
            self.status = RoomStatus::Ready;
            return Ok(false);
        }

        self.status = RoomStatus::Playing;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| slot.status = PlayerStatus::Playing);

        Ok(true)
    }

    /// Validates host snapshot chunk relay.
    pub fn accept_snapshot_chunk(
        &mut self,
        connection_id: ConnectionId,
        chunk: &SnapshotChunk,
        limits: SnapshotLimits,
    ) -> Result<(), RoomError> {
        self.validate_host_snapshot_sender(connection_id)?;
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
        let transfer = self
            .snapshot_transfer
            .as_ref()
            .ok_or(RoomError::SnapshotInvalid)?;
        transfer.complete(manifest, limits)?;
        self.snapshot_transfer = None;
        self.host_snapshot_completed = true;

        Ok(())
    }

    /// Validates and records an input frame from one connection.
    pub fn accept_input_frame(
        &mut self,
        connection_id: ConnectionId,
        input: &InputFrame,
        limits: InputFrameLimits,
    ) -> Result<(), RoomError> {
        if self.session.mode != NetplaySessionMode::ControllerNetplay {
            return Err(RoomError::NotPlaying);
        }

        if self.status != RoomStatus::Playing {
            return Err(RoomError::NotPlaying);
        }

        let owned_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        if owned_index != input.player_index {
            return Err(RoomError::SlotSpoofing(input.player_index));
        }

        if let Some(last_frame) = self.last_input_frames.get(&input.player_index)
            && input.frame <= *last_frame
        {
            return Err(RoomError::OutOfOrderFrame);
        }

        if input.frame > self.room_frame + limits.max_future_frame_distance {
            return Err(RoomError::FutureFrameTooLarge);
        }

        self.last_input_frames
            .insert(input.player_index, input.frame);
        self.recompute_room_frame();

        Ok(())
    }

    /// Creates a serializable view for HTTP and WebSocket responses.
    pub fn view(&self) -> RoomView {
        RoomView {
            room_id: self.room_id,
            invite_code: self.invite_code.display(),
            protocol: NetplayProtocolView::default(),
            session: self.session.clone(),
            max_players: self.max_players,
            status: self.status,
            players: self
                .players
                .iter()
                .map(|slot| PlayerSlotView {
                    player_index: slot.player_index.zero_based(),
                    display_number: slot.player_index.display_number(),
                    role: slot.role,
                    status: slot.status,
                    occupied: !slot.is_empty(),
                })
                .collect(),
        }
    }

    fn connected_players_have_fingerprints(&self) -> bool {
        let connected_players = self.connected_player_indices();

        connected_players.len() == usize::from(self.max_players)
            && connected_players
                .iter()
                .all(|player_index| self.compatibility.contains_key(player_index))
    }

    pub(super) fn player_index_for_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Option<PlayerIndex> {
        self.players
            .iter()
            .find(|slot| slot.connection_id == Some(connection_id))
            .map(|slot| slot.player_index)
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

    pub(super) fn connected_player_indices(&self) -> Vec<PlayerIndex> {
        self.players
            .iter()
            .filter(|slot| slot.connection_id.is_some())
            .map(|slot| slot.player_index)
            .collect()
    }

    fn connected_players_are_ready(&self) -> bool {
        let connected_players = self.connected_player_indices();

        connected_players.len() == usize::from(self.max_players)
            && connected_players
                .iter()
                .all(|player_index| self.ready_players.contains(player_index))
    }

    pub(super) fn set_player_status(&mut self, player_index: PlayerIndex, status: PlayerStatus) {
        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
        {
            slot.status = status;
        }
    }

    fn recompute_room_frame(&mut self) {
        if let Some(min_frame) = self.last_input_frames.values().min().copied() {
            self.room_frame = min_frame;
        }
    }

    fn reset_sync_state(&mut self) {
        self.compatibility.clear();
        self.ready_players.clear();
        self.last_input_frames.clear();
        self.link_cable_state.reset();
        self.host_snapshot_completed = false;
        self.snapshot_transfer = None;
        self.room_frame = 0;
    }

    fn fingerprint_matches_session(&self, fingerprint: &CompatibilityFingerprint) -> bool {
        fingerprint.protocol_version == crate::protocol::NETPLAY_PROTOCOL_VERSION
            && fingerprint.system_id == self.session.game.system_id
            && fingerprint.core_id == self.session.core.core_id
            && self.fingerprint_state_format_matches_session(fingerprint)
            && fingerprint
                .content_hash
                .eq_ignore_ascii_case(&self.session.game.rom_sha256)
    }

    fn fingerprint_state_format_matches_session(
        &self,
        fingerprint: &CompatibilityFingerprint,
    ) -> bool {
        match self.session.core.state_format.as_deref() {
            Some(state_format) => fingerprint.state_format.as_deref() == Some(state_format),
            None => true,
        }
    }
}

#[cfg(test)]
#[path = "room_tests.rs"]
mod room_tests;
