//! Room-owned state for large snapshot file-relay transfers.
//!
//! The relay does not store snapshot bytes. It records only the active transfer
//! metadata needed to validate completion and send the receiver's private grant.

use crate::protocol::{SnapshotFileRelayGrant, SnapshotManifest};
use crate::rooms::{ConnectionId, PlayerIndex, RoomId};

/// Validated room data needed before calling the trusted file-relay service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFileRelayTransferIntent {
    /// Room id that owns the temporary transfer.
    pub room_id: RoomId,
    /// Player that will upload the payload.
    pub sender_player_index: PlayerIndex,
    /// Player that will download the payload.
    pub receiver_player_index: PlayerIndex,
    /// Receiver connection active when the grant is created.
    pub receiver_connection: ConnectionId,
}

/// Validated file-relay transfer that is waiting for host upload completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFileRelayTransferState {
    /// Host connection that requested the transfer.
    pub source_connection: ConnectionId,
    /// Player that will receive the payload.
    pub receiver_player_index: PlayerIndex,
    /// Grant held until the host confirms upload completion.
    pub download_grant: SnapshotFileRelayGrant,
    /// Manifest the transfer must satisfy.
    pub manifest: SnapshotManifest,
}

impl SnapshotFileRelayTransferState {
    /// Creates a pending transfer record.
    pub fn new(
        source_connection: ConnectionId,
        receiver_player_index: PlayerIndex,
        download_grant: SnapshotFileRelayGrant,
        manifest: SnapshotManifest,
    ) -> Self {
        Self {
            source_connection,
            receiver_player_index,
            download_grant,
            manifest,
        }
    }
}
