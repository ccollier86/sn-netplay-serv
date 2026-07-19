//! Domain model for one active netplay room.
//!
//! This module owns slot assignment, room status transitions, compatibility
//! checks, and input-frame validation. It does not store rooms globally or
//! perform network IO.

use crate::auth::VerifiedLicense;
use crate::limits::MVP_ROOM_CAPACITY;
use crate::protocol::{
    ClientNetworkQualityReport, CompatibilityFingerprint, InputDelayChange, NetplayProtocolView,
    NetplaySessionDescriptor, NetplaySessionMode, ScheduledSessionStart, StateDigestMode,
};
use crate::rooms::{
    AdaptiveInputDelayPolicy, ClockSyncSampleRequestState, ConnectionId, InviteCode,
    LinkCableRoomState, PlayerIndex, PlayerRuntimeState, PlayerSlot, PlayerSlotView, PlayerStatus,
    ResumeTokenHash, RomRelayTransferState, RoomError, RoomId, RoomStatus, RoomView,
    RoomVoiceState, SessionPauseStateTracker, SnapshotFileRelayTransferState,
    SnapshotTransferState,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Instant;

/// Active netplay room.
#[derive(Clone, Debug)]
pub struct NetplayRoom {
    room_id: RoomId,
    invite_code: InviteCode,
    protocol_version: u16,
    pub(super) session: NetplaySessionDescriptor,
    pub(super) max_players: u8,
    pub(super) players: Vec<PlayerSlot>,
    pub(super) status: RoomStatus,
    pub(super) room_epoch: u64,
    pub(super) session_epoch: u64,
    pub(super) compatibility: HashMap<PlayerIndex, CompatibilityFingerprint>,
    pub(super) ready_players: HashSet<PlayerIndex>,
    pub(super) deterministic_ready_players: HashSet<PlayerIndex>,
    pub(super) clock_sync_request: Option<ClockSyncSampleRequestState>,
    pub(super) clock_uncertainty_by_player: HashMap<PlayerIndex, u64>,
    pub(super) clock_sample_indices_by_player: HashMap<PlayerIndex, BTreeSet<u8>>,
    pub(super) next_clock_sync_request_id: u64,
    pub(super) last_input_frames: HashMap<PlayerIndex, u64>,
    pub(super) next_input_frames: HashMap<PlayerIndex, u64>,
    pub(super) link_cable_state: LinkCableRoomState,
    pub(super) host_snapshot_completed: bool,
    pub(super) next_pause_sequence: u64,
    pub(super) pause_state: Option<SessionPauseStateTracker>,
    pub(super) snapshot_transfer: Option<SnapshotTransferState>,
    pub(super) snapshot_file_relay_transfer: Option<SnapshotFileRelayTransferState>,
    pub(super) rom_relay_transfer: Option<RomRelayTransferState>,
    pub(super) sync_start_frame: u64,
    pub(super) room_frame: u64,
    pub(super) released_frame: Option<u64>,
    pub(super) next_release_frame: u64,
    pub(super) pending_input_delay_change: Option<InputDelayChange>,
    pub(super) input_delay_policy: AdaptiveInputDelayPolicy,
    pub(super) state_hashes: BTreeMap<u64, HashMap<PlayerIndex, String>>,
    pub(super) state_hash_true_mismatch_streak: u8,
    pub(super) scheduled_start: Option<ScheduledSessionStart>,
    pub(super) pending_host_frame_open: Option<u64>,
    pub(super) voice: Option<RoomVoiceState>,
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
        Self::new_with_protocol_and_resume(
            host,
            host_connection,
            invite_code,
            session,
            crate::protocol::LEGACY_NETPLAY_PROTOCOL_VERSION,
            host_resume_token_hash,
            host_input_socket_token_hash,
            now,
        )
    }

    /// Creates a room with an exact negotiated protocol and resume capabilities.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_protocol_and_resume(
        host: VerifiedLicense,
        host_connection: ConnectionId,
        invite_code: InviteCode,
        session: NetplaySessionDescriptor,
        protocol_version: u16,
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
            protocol_version,
            session,
            max_players,
            players,
            status: RoomStatus::WaitingForGuest,
            room_epoch: 1,
            session_epoch: 1,
            compatibility: HashMap::new(),
            ready_players: HashSet::new(),
            deterministic_ready_players: HashSet::new(),
            clock_sync_request: None,
            clock_uncertainty_by_player: HashMap::new(),
            clock_sample_indices_by_player: HashMap::new(),
            next_clock_sync_request_id: 1,
            last_input_frames: HashMap::new(),
            next_input_frames: HashMap::new(),
            link_cable_state: LinkCableRoomState::default(),
            host_snapshot_completed: false,
            next_pause_sequence: 1,
            pause_state: None,
            snapshot_transfer: None,
            snapshot_file_relay_transfer: None,
            rom_relay_transfer: None,
            sync_start_frame: 0,
            room_frame: 0,
            released_frame: None,
            next_release_frame: 0,
            pending_input_delay_change: None,
            input_delay_policy: AdaptiveInputDelayPolicy::new(now),
            state_hashes: BTreeMap::new(),
            state_hash_true_mismatch_streak: 0,
            scheduled_start: None,
            pending_host_frame_open: None,
            voice: None,
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

    /// Returns the exact wire protocol selected when this room was created.
    pub fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    /// Returns whether this room uses strict controller input and host opens.
    pub fn uses_strict_controller_input(&self) -> bool {
        self.protocol_version >= 5 && self.session.mode == NetplaySessionMode::ControllerNetplay
    }

    /// Returns the state-digest authority negotiated by this room.
    pub(crate) fn state_digest_mode(&self) -> StateDigestMode {
        if !self.uses_strict_controller_input() {
            return StateDigestMode::Authoritative;
        }

        self.compatibility
            .values()
            .find_map(|fingerprint| fingerprint.valid_determinism_v5())
            .map_or(StateDigestMode::Disabled, |profile| profile.digest_mode)
    }

    /// Returns the configured player capacity.
    pub(crate) fn max_players(&self) -> u8 {
        self.max_players
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
            self.snapshot_file_relay_transfer = None;
            self.rom_relay_transfer = None;
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

        if self.connected_players_support_scheduled_start() {
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

    /// Returns the canonical frame that the current sync phase will start from.
    pub(super) fn sync_start_frame(&self) -> u64 {
        self.sync_start_frame
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
            protocol: NetplayProtocolView::for_room(self.protocol_version),
            session: self.session.clone(),
            voice: self.voice.as_ref().map(RoomVoiceState::view),
            rom_relay: self.session.rom_relay.clone(),
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
                    supports_state_file_relay: slot.supports_state_file_relay,
                    supports_rom_file_relay: slot.supports_rom_file_relay,
                    supports_scheduled_start: slot.supports_scheduled_start,
                    supports_clock_sync: slot.supports_clock_sync,
                    supports_fast_input_relay: slot.supports_fast_input_relay,
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
        self.reset_sync_state_to(0);
    }

    pub(super) fn reset_sync_state_to(&mut self, start_frame: u64) {
        self.compatibility.clear();
        self.ready_players.clear();
        self.last_input_frames.clear();
        self.next_input_frames.clear();
        self.link_cable_state.reset();
        self.host_snapshot_completed = false;
        self.pause_state = None;
        self.snapshot_transfer = None;
        self.snapshot_file_relay_transfer = None;
        self.rom_relay_transfer = None;
        self.sync_start_frame = start_frame;
        self.room_frame = start_frame;
        self.released_frame = None;
        self.next_release_frame = start_frame;
        self.pending_input_delay_change = None;
        self.state_hashes.clear();
        self.state_hash_true_mismatch_streak = 0;
        self.pending_host_frame_open = None;
        self.reset_start_sync_state();
    }

    pub(super) fn bump_room_epoch(&mut self) {
        self.room_epoch = self.room_epoch.saturating_add(1);
    }

    pub(super) fn bump_session_epoch(&mut self) {
        self.session_epoch = self.session_epoch.saturating_add(1);
    }

    fn fingerprint_matches_session(&self, fingerprint: &CompatibilityFingerprint) -> bool {
        let legacy_fields_match = fingerprint.protocol_version == self.protocol_version
            && fingerprint.system_id == self.session.game.system_id
            && fingerprint.core_id == self.session.core.core_id
            && self.fingerprint_state_format_matches_session(fingerprint)
            && fingerprint
                .content_hash
                .eq_ignore_ascii_case(&self.session.game.rom_sha256);

        if !legacy_fields_match || !self.uses_strict_controller_input() {
            return legacy_fields_match;
        }

        let Some(profile) = fingerprint.valid_determinism_v5() else {
            return false;
        };
        let Some(rom_identity) = self.session.rom_identity.as_ref() else {
            return false;
        };

        profile.rom_size_bytes == rom_identity.size_bytes
            && fingerprint
                .settings_hash
                .eq_ignore_ascii_case(&profile.core_options_digest)
            && self
                .session
                .core
                .core_options_sha256
                .as_deref()
                .is_none_or(|digest| digest.eq_ignore_ascii_case(&profile.core_options_digest))
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
