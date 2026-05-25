//! DTOs for the trusted file relay service.
//!
//! These mirror `sb-file-relay-serv` without coupling the netplay relay to that
//! server's crate.

use serde::{Deserialize, Serialize};

/// Payload kind sent through the file relay.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileRelayTransferKind {
    /// Temporary ROM payload.
    Rom,
    /// Serialized save-state payload.
    SaveState,
}

/// Service-created transfer request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileRelayTransferRequest {
    /// Netplay room or lobby id that owns this transfer.
    pub room_id: String,
    /// Sender player id or slot id.
    pub sender_player_id: String,
    /// Receiver player id or slot id.
    pub receiver_player_id: String,
    /// Payload kind.
    pub kind: FileRelayTransferKind,
    /// Expected SHA-256 of the complete payload.
    pub sha256: String,
    /// Expected byte size of the complete payload.
    pub size_bytes: u64,
    /// Optional shorter lifetime than the relay default.
    pub expires_in_seconds: Option<u64>,
}

/// File relay transfer grant returned by the trusted relay.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileRelayTransferResponse {
    /// Generated transfer id.
    pub transfer_id: String,
    /// Bytes per chunk.
    pub chunk_size_bytes: u64,
    /// Expected chunk count.
    pub chunk_count: u64,
    /// Opaque upload token for the sender.
    pub upload_token: String,
    /// Opaque download token for the receiver.
    pub download_token: String,
    /// Transfer expiry timestamp.
    pub expires_at: String,
}
