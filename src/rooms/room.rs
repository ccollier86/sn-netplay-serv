//! Domain model for one active netplay room.
//!
//! This module owns slot assignment, room status transitions, compatibility
//! checks, and input-frame validation. It does not store rooms globally or
//! perform network IO.

use crate::auth::VerifiedLicense;
use crate::limits::MVP_ROOM_CAPACITY;
use crate::protocol::{
    ClientNetworkQualityReport, CompatibilityFingerprint, InputDelayChange, NetplayProtocolView,
    NetplaySessionDescriptor, NetplaySessionMode, SnapshotChunk, SnapshotLimits, SnapshotManifest,
};
use crate::rooms::{
    AdaptiveInputDelayPolicy, ConnectionId, InviteCode, LinkCableRoomState, PlayerIndex,
    PlayerRole, PlayerRuntimeState, PlayerSlot, PlayerSlotView, PlayerStatus, ResumeTokenHash,
    RoomError, RoomId, RoomStatus, RoomView, SessionPauseStateTracker, SnapshotTransferState,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

/// Active netplay room.
#[derive(Clone, Debug)]
pub struct NetplayRoom {
    room_id: RoomId,
    invite_code: InviteCode,
    pub(super) session: NetplaySessionDescriptor,
    pub(super) max_players: u8,
    pub(super) players: Vec<PlayerSlot>,
    pub(super) status: RoomStatus,
    pub(super) room_epoch: u64,
    pub(super) session_epoch: u64,
    pub(super) compatibility: HashMap<PlayerIndex, CompatibilityFingerprint>,
    pub(super) ready_players: HashSet<PlayerIndex>,
    pub(super) last_input_frames: HashMap<PlayerIndex, u64>,
    pub(super) link_cable_state: LinkCableRoomState,
    host_snapshot_completed: bool,
    pub(super) next_pause_sequence: u64,
    pub(super) pause_state: Option<SessionPauseStateTracker>,
    snapshot_transfer: Option<SnapshotTransferState>,
    pub(super) room_frame: u64,
    pub(super) released_frame: Option<u64>,
    pub(super) next_release_frame: u64,
    pub(super) pending_input_delay_change: Option<InputDelayChange>,
    pub(super) input_delay_policy: AdaptiveInputDelayPolicy,
    pub(super) state_hashes: BTreeMap<u64, HashMap<PlayerIndex, String>>,
}

impl NetplayRoom {
    /// Creates a room and reserves Player 1 for the verified host.
    pub fn new(
        host: VerifiedLicense,
        host_connection: ConnectionId,
        invite_code: InviteCode,
        session: NetplaySessionDescriptor,
    ) -> Self {
        Self::new_with_resume(
            host,
            host_connection,
            invite_code,
            session,
            String::new(),
            String::new(),
            Instant::now(),
        )
    }

    /// Creates a room with an explicit host resume-token hash.
    pub fn new_with_resume(
        host: VerifiedLicense,
        host_connection: ConnectionId,
        invite_code: InviteCode,
        session: NetplaySessionDescriptor,
        host_resume_token_hash: ResumeTokenHash,
        host_input_socket_token_hash: ResumeTokenHash,
        now: Instant,
    ) -> Self {
        let max_players = MVP_ROOM_CAPACITY;
        let mut players = Vec::with_capacity(usize::from(max_players));
        players.push(PlayerSlot::host(
            &host,
            host_connection,
            host_resume_token_hash,
            host_input_socket_token_hash,
            now,
        ));

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
            room_epoch: 1,
            session_epoch: 1,
            compatibility: HashMap::new(),
            ready_players: HashSet::new(),
            last_input_frames: HashMap::new(),
            link_cable_state: LinkCableRoomState::default(),
            host_snapshot_completed: false,
            next_pause_sequence: 1,
            pause_state: None,
            snapshot_transfer: None,
            room_frame: 0,
            released_frame: None,
            next_release_frame: 0,
            pending_input_delay_change: None,
            input_delay_policy: AdaptiveInputDelayPolicy::new(now),
            state_hashes: BTreeMap::new(),
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
                .for_each(|slot| {
                    slot.status = PlayerStatus::CompatibilityFailed;
                    slot.runtime_state = PlayerRuntimeState::Connected;
                });
            return Err(RoomError::CompatibilityMismatch);
        }

        self.status = RoomStatus::SyncingState;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::SyncingState;
                slot.runtime_state = PlayerRuntimeState::Syncing;
            });

        Ok(())
    }

    /// Marks a connected player ready and starts when every player is ready.
    pub fn mark_ready(
        &mut self,
        connection_id: ConnectionId,
        network: Option<ClientNetworkQualityReport>,
        now: Instant,
    ) -> Result<bool, RoomError> {
        if self.status != RoomStatus::SyncingState && self.status != RoomStatus::Ready {
            return Err(RoomError::RoomNotReady);
        }

        if self.session.mode == NetplaySessionMode::ControllerNetplay
            && !self.host_snapshot_completed
        {
            return Err(RoomError::RoomNotReady);
        }

        if self.session.mode == NetplaySessionMode::ControllerNetplay
            && !self.connected_players_have_input_sockets()
        {
            return Err(RoomError::RoomNotReady);
        }

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        self.record_network_report(connection_id, None, network, now);
        self.ready_players.insert(player_index);
        self.set_player_status(player_index, PlayerStatus::Ready);

        if !self.connected_players_are_ready() {
            self.status = RoomStatus::Ready;
            return Ok(false);
        }

        self.apply_initial_adaptive_input_delay(now);
        self.status = RoomStatus::Playing;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Playing;
                slot.runtime_state = PlayerRuntimeState::Playing;
            });

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

    /// Creates a serializable view for HTTP and WebSocket responses.
    pub fn view(&self) -> RoomView {
        self.view_for_event(0, Instant::now())
    }

    /// Creates a serializable view with relay event metadata.
    pub fn view_for_event(&self, event_seq: u64, now: Instant) -> RoomView {
        RoomView {
            room_id: self.room_id,
            event_seq,
            room_epoch: self.room_epoch,
            session_epoch: self.session_epoch,
            invite_code: self.invite_code.display(),
            protocol: NetplayProtocolView::default(),
            session: self.session.clone(),
            max_players: self.max_players,
            pause: self
                .pause_state
                .as_ref()
                .map(|pause_state| pause_state.view(self.current_pause_state())),
            frame_clock: self.frame_clock_view(),
            status: self.status,
            players: self
                .players
                .iter()
                .map(|slot| PlayerSlotView {
                    player_index: slot.player_index.zero_based(),
                    display_number: slot.player_index.display_number(),
                    role: slot.role,
                    status: slot.status,
                    runtime_state: slot.runtime_state,
                    occupied: !slot.is_empty(),
                    control_connected: slot.connection_id.is_some(),
                    input_connected: slot.input_connection_id.is_some(),
                    last_seen_age_ms: slot
                        .last_seen_at
                        .map(|last_seen| now.saturating_duration_since(last_seen).as_millis()),
                    reconnect_grace_remaining_ms: slot
                        .reconnect_deadline
                        .map(|deadline| deadline.saturating_duration_since(now).as_millis()),
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

    fn connected_players_have_input_sockets(&self) -> bool {
        let connected_players = self.connected_player_indices();

        connected_players.len() == usize::from(self.max_players)
            && connected_players.iter().all(|player_index| {
                self.players
                    .iter()
                    .find(|slot| slot.player_index == *player_index)
                    .is_some_and(|slot| slot.input_connection_id.is_some())
            })
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

    pub(super) fn player_index_for_input_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Option<PlayerIndex> {
        self.players
            .iter()
            .find(|slot| slot.input_connection_id == Some(connection_id))
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
            slot.runtime_state = match status {
                PlayerStatus::Empty => PlayerRuntimeState::Empty,
                PlayerStatus::Connected => PlayerRuntimeState::Connected,
                PlayerStatus::CheckingCompatibility => PlayerRuntimeState::CheckingCompatibility,
                PlayerStatus::CompatibilityFailed => PlayerRuntimeState::Connected,
                PlayerStatus::SyncingState => PlayerRuntimeState::Syncing,
                PlayerStatus::Ready => PlayerRuntimeState::Ready,
                PlayerStatus::Playing => PlayerRuntimeState::Playing,
                PlayerStatus::Paused => PlayerRuntimeState::Paused,
                PlayerStatus::Reconnecting => PlayerRuntimeState::Reconnecting,
                PlayerStatus::RecoveryExpired => PlayerRuntimeState::RecoveryExpired,
                PlayerStatus::Disconnected => PlayerRuntimeState::Disconnected,
            };
        }
    }

    pub(super) fn reset_sync_state(&mut self) {
        self.compatibility.clear();
        self.ready_players.clear();
        self.last_input_frames.clear();
        self.link_cable_state.reset();
        self.host_snapshot_completed = false;
        self.pause_state = None;
        self.snapshot_transfer = None;
        self.room_frame = 0;
        self.released_frame = None;
        self.next_release_frame = 0;
        self.pending_input_delay_change = None;
        self.state_hashes.clear();
    }

    pub(super) fn bump_room_epoch(&mut self) {
        self.room_epoch = self.room_epoch.saturating_add(1);
    }

    pub(super) fn bump_session_epoch(&mut self) {
        self.session_epoch = self.session_epoch.saturating_add(1);
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
