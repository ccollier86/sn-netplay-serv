//! Lobby file-relay protocol values.
//!
//! These grants are private per lobby socket. They let the lobby relay
//! coordinate temporary session payloads without moving large bytes over the
//! lobby WebSocket.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Token role granted to one lobby client for a temporary transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyFileRelayGrantRole {
    /// Sender may upload the payload bytes.
    Upload,
    /// Receiver may download the payload bytes.
    Download,
}

/// Type of lobby prelaunch material moved through the file relay.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyFileRelayMaterialKind {
    /// Selected game content required before launch.
    Game,
    /// Selected startup save-state material required before launch.
    StartupState,
}

/// Startup restore policy attached to a relayed startup state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum LobbyStartupStateRestorePolicy {
    /// Load the startup state immediately.
    #[serde(rename = "immediate")]
    Immediate,
    /// Wait the requested number of normal frames before loading the state.
    #[serde(rename = "afterFrames")]
    AfterFrames {
        /// Normal frames to run before restore.
        frames: u32,
    },
}

/// Sender-provided startup-state material metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyStartupStateTransferMetadata {
    /// Expected complete startup-state SHA-256.
    pub sha256: String,
    /// Expected complete startup-state byte size.
    pub size_bytes: u64,
    /// Safe user-facing startup-state label.
    #[serde(default)]
    pub label: Option<String>,
    /// Restore timing policy needed by the selected core.
    pub restore_policy: LobbyStartupStateRestorePolicy,
    /// Optional state-format identity for future cross-platform checks.
    #[serde(default)]
    pub state_format: Option<String>,
}

/// Private file-relay grant for a lobby-scoped transfer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyFileRelayGrant {
    /// File relay transfer id.
    pub transfer_id: String,
    /// Public file relay base URL clients should call.
    pub relay_url: String,
    /// Opaque upload or download bearer token.
    pub token: String,
    /// Whether this grant uploads or downloads the payload.
    pub role: LobbyFileRelayGrantRole,
    /// Type of lobby material this grant transfers.
    #[serde(default = "default_lobby_file_relay_material_kind")]
    pub material_kind: LobbyFileRelayMaterialKind,
    /// Selected game proposal this transfer belongs to.
    pub proposal_id: Uuid,
    /// Zero-based sender player index.
    pub sender_player_index: u8,
    /// Zero-based receiver player index.
    pub receiver_player_index: u8,
    /// Expected complete payload SHA-256.
    pub sha256: String,
    /// Expected complete payload byte size.
    pub size_bytes: u64,
    /// File relay chunk size.
    pub chunk_size_bytes: u64,
    /// Number of chunks expected for this transfer.
    pub chunk_count: u64,
    /// Transfer expiry timestamp from the file relay.
    pub expires_at: String,
    /// Startup-state metadata when this grant transfers selected startup state.
    #[serde(default)]
    pub startup_state: Option<LobbyStartupStateTransferMetadata>,
}

/// Pair of private grants for one lobby transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyFileRelayGrantPair {
    /// Grant sent privately to the sender.
    pub upload: LobbyFileRelayGrant,
    /// Grant sent privately to the receiver.
    pub download: LobbyFileRelayGrant,
}

fn default_lobby_file_relay_material_kind() -> LobbyFileRelayMaterialKind {
    LobbyFileRelayMaterialKind::Game
}
