//! File-relay snapshot transfer protocol values.
//!
//! These messages are additive to the inline `snapshotChunk` flow. Clients that
//! do not explicitly advertise support never receive these messages.

use crate::protocol::SnapshotManifest;
use serde::{Deserialize, Serialize};

/// Token role granted to one client for a temporary snapshot transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotFileRelayGrantRole {
    /// Sender may upload the snapshot bytes.
    Upload,
    /// Receiver may download the snapshot bytes.
    Download,
}

/// Client-specific temporary file relay grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFileRelayGrant {
    /// File relay transfer id.
    pub transfer_id: String,
    /// Public file relay base URL clients should call.
    pub relay_url: String,
    /// Opaque upload or download bearer token.
    pub token: String,
    /// Whether this grant uploads or downloads the payload.
    pub role: SnapshotFileRelayGrantRole,
    /// File relay chunk size.
    pub chunk_size_bytes: u64,
    /// Number of chunks expected for this transfer.
    pub chunk_count: u64,
    /// Transfer expiry timestamp from the file relay.
    pub expires_at: String,
    /// Snapshot metadata the payload must satisfy.
    pub manifest: SnapshotManifest,
}

/// Pair of grants created for one host-to-peer snapshot transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFileRelayGrantPair {
    /// Grant sent privately to the host.
    pub upload: SnapshotFileRelayGrant,
    /// Grant sent privately to the receiver.
    pub download: SnapshotFileRelayGrant,
}
