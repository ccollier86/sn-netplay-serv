//! Player slot state for active rooms.
//!
//! Slots are capacity-based instead of host/guest fields so the protocol can
//! expand past two players later without changing its shape.

use crate::auth::VerifiedLicense;
use crate::protocol::ClientNetworkQualityReport;
use crate::rooms::{ConnectionId, PlayerIndex, ResumeTokenHash};
use serde::Serialize;
use std::time::Instant;

/// Role assigned by the server when a player joins.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerRole {
    /// Room creator and Player 1 for the MVP.
    Host,
    /// Joined player.
    Guest,
}

/// User-facing player status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerStatus {
    /// No player occupies this slot.
    Empty,
    /// Player is connected to the room.
    Connected,
    /// Player is comparing compatibility fingerprints.
    CheckingCompatibility,
    /// Player failed compatibility checks.
    CompatibilityFailed,
    /// Player is receiving or sending sync state.
    SyncingState,
    /// Player is ready to start.
    Ready,
    /// Player is in active gameplay.
    Playing,
    /// Player is paused by coordinated netplay pause.
    Paused,
    /// Player may reconnect to reclaim this slot.
    Reconnecting,
    /// Player failed to reconnect before the recovery window expired.
    RecoveryExpired,
    /// Player disconnected.
    Disconnected,
}

/// Runtime state reported or inferred for one player slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerRuntimeState {
    /// No player occupies this slot.
    Empty,
    /// Socket is connected but gameplay is not active.
    Connected,
    /// Compatibility check is in progress.
    CheckingCompatibility,
    /// Snapshot or link setup is in progress.
    Syncing,
    /// Client is ready to start.
    Ready,
    /// Gameplay is active.
    Playing,
    /// A coordinated pause has been scheduled.
    Pausing,
    /// Client is paused at a coordinated frame.
    Paused,
    /// Socket dropped and the slot is waiting for reconnect.
    Reconnecting,
    /// Heartbeat is stale but the socket is not in recovery yet.
    Stale,
    /// Socket is disconnected without a recoverable session.
    Disconnected,
    /// Reconnect grace expired.
    RecoveryExpired,
}

/// Slot assigned to one player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSlot {
    /// Zero-based player index used by protocol messages.
    pub player_index: PlayerIndex,
    /// Role assigned by the server.
    pub role: PlayerRole,
    /// Verified client identity occupying the slot.
    pub subject_key: Option<String>,
    /// Active socket connection occupying the slot.
    pub connection_id: Option<ConnectionId>,
    /// Active binary input socket for this slot.
    pub input_connection_id: Option<ConnectionId>,
    /// Optional name shown in Desktop room UI.
    pub display_name: Option<String>,
    /// Current lifecycle status.
    pub status: PlayerStatus,
    /// Current emulator/runtime status.
    pub runtime_state: PlayerRuntimeState,
    /// Hash of the per-player token used to reclaim this slot.
    pub resume_token_hash: Option<ResumeTokenHash>,
    /// Hash of the token used to attach the binary input socket.
    pub input_socket_token_hash: Option<ResumeTokenHash>,
    /// Last heartbeat time seen by the relay.
    pub last_seen_at: Option<Instant>,
    /// Latest local runtime frame reported by this client.
    pub latest_local_frame: Option<u64>,
    /// Time the latest local runtime frame was reported.
    pub latest_local_frame_reported_at: Option<Instant>,
    /// Latest client-observed network/runtime health sample.
    pub latest_network_report: Option<ClientNetworkQualityReport>,
    /// Whether this control socket can use file relay for large sync states.
    pub supports_state_file_relay: bool,
    /// Time the latest network/runtime health sample was reported.
    pub latest_network_reported_at: Option<Instant>,
    /// Deadline for reclaiming this slot after transport loss.
    pub reconnect_deadline: Option<Instant>,
    /// Room epoch this client knew before recovery changed room state.
    pub reconnect_room_epoch: Option<u64>,
}

impl PlayerSlot {
    /// Creates an empty slot for `player_index`.
    pub fn empty(player_index: PlayerIndex) -> Self {
        Self {
            player_index,
            role: PlayerRole::Guest,
            subject_key: None,
            connection_id: None,
            input_connection_id: None,
            display_name: None,
            status: PlayerStatus::Empty,
            runtime_state: PlayerRuntimeState::Empty,
            resume_token_hash: None,
            input_socket_token_hash: None,
            last_seen_at: None,
            latest_local_frame: None,
            latest_local_frame_reported_at: None,
            latest_network_report: None,
            supports_state_file_relay: false,
            latest_network_reported_at: None,
            reconnect_deadline: None,
            reconnect_room_epoch: None,
        }
    }

    /// Creates the host slot from a verified license.
    pub fn host(
        license: &VerifiedLicense,
        connection_id: ConnectionId,
        resume_token_hash: ResumeTokenHash,
        input_socket_token_hash: ResumeTokenHash,
        now: Instant,
    ) -> Self {
        Self {
            player_index: PlayerIndex::ONE,
            role: PlayerRole::Host,
            subject_key: Some(license.identity_key()),
            connection_id: Some(connection_id),
            input_connection_id: None,
            display_name: None,
            status: PlayerStatus::Connected,
            runtime_state: PlayerRuntimeState::Connected,
            resume_token_hash: Some(resume_token_hash),
            input_socket_token_hash: Some(input_socket_token_hash),
            last_seen_at: Some(now),
            latest_local_frame: None,
            latest_local_frame_reported_at: None,
            latest_network_report: None,
            supports_state_file_relay: false,
            latest_network_reported_at: None,
            reconnect_deadline: None,
            reconnect_room_epoch: None,
        }
    }

    /// Marks an empty slot as occupied by a guest.
    pub fn occupy_guest(
        &mut self,
        license: &VerifiedLicense,
        connection_id: ConnectionId,
        resume_token_hash: ResumeTokenHash,
        input_socket_token_hash: ResumeTokenHash,
        now: Instant,
        supports_state_file_relay: bool,
    ) {
        self.role = PlayerRole::Guest;
        self.subject_key = Some(license.identity_key());
        self.connection_id = Some(connection_id);
        self.input_connection_id = None;
        self.status = PlayerStatus::Connected;
        self.runtime_state = PlayerRuntimeState::Connected;
        self.resume_token_hash = Some(resume_token_hash);
        self.input_socket_token_hash = Some(input_socket_token_hash);
        self.last_seen_at = Some(now);
        self.latest_local_frame = None;
        self.latest_local_frame_reported_at = None;
        self.latest_network_report = None;
        self.supports_state_file_relay = supports_state_file_relay;
        self.latest_network_reported_at = None;
        self.reconnect_deadline = None;
        self.reconnect_room_epoch = None;
    }

    /// Returns whether the slot is available.
    pub fn is_empty(&self) -> bool {
        self.status == PlayerStatus::Empty
    }
}
