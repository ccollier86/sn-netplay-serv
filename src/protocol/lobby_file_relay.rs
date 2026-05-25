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
}

/// Pair of private grants for one lobby transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyFileRelayGrantPair {
    /// Grant sent privately to the sender.
    pub upload: LobbyFileRelayGrant,
    /// Grant sent privately to the receiver.
    pub download: LobbyFileRelayGrant,
}
