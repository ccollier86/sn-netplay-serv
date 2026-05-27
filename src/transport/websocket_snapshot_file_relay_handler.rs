//! WebSocket handling for large snapshot file-relay requests.
//!
//! This module translates validated room snapshot intent into trusted
//! file-relay grants. Inline snapshot chunks remain handled by the generic
//! message handler.

use crate::file_relay::{CreateFileRelayTransferRequest, FileRelayTransferKind};
use crate::http::AppServices;
use crate::protocol::{
    SnapshotFileRelayGrant, SnapshotFileRelayGrantPair, SnapshotFileRelayGrantRole,
    SnapshotManifest,
};
use crate::rooms::{ConnectionId, InviteCode, RoomError, SnapshotFileRelayTransferIntent};
use tracing::warn;

const SNAPSHOT_FILE_RELAY_TTL_SECONDS: u64 = 180;

/// Creates a temporary file-relay transfer for a host snapshot.
pub async fn handle_snapshot_file_relay_request(
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    manifest: SnapshotManifest,
) -> Result<(), RoomError> {
    if !services
        .file_relay_policy
        .can_relay_save_states(services.file_relay.as_ref())
    {
        return Err(RoomError::SnapshotFileRelayUnavailable);
    }

    let relay_url = services
        .file_relay
        .public_base_url()
        .ok_or(RoomError::SnapshotFileRelayUnavailable)?
        .to_string();

    let intent = services
        .rooms
        .prepare_snapshot_file_relay(invite_code.clone(), connection_id, manifest.clone())
        .await?;
    let response = services
        .file_relay
        .create_transfer(create_snapshot_file_relay_request(&intent, &manifest))
        .await
        .map_err(|error| {
            warn!(error = %error, "snapshot file relay transfer creation failed");
            RoomError::SnapshotFileRelayUnavailable
        })?;
    let grants = snapshot_file_relay_grants(response, relay_url, manifest.clone());

    services
        .rooms
        .grant_snapshot_file_relay_upload(invite_code.clone(), connection_id, manifest, grants)
        .await
}

fn create_snapshot_file_relay_request(
    intent: &SnapshotFileRelayTransferIntent,
    manifest: &SnapshotManifest,
) -> CreateFileRelayTransferRequest {
    CreateFileRelayTransferRequest {
        room_id: intent.room_id.to_string(),
        sender_player_id: format!("player-{}", intent.sender_player_index.display_number()),
        receiver_player_id: format!("player-{}", intent.receiver_player_index.display_number()),
        kind: FileRelayTransferKind::SaveState,
        sha256: manifest.sha256.clone(),
        size_bytes: manifest.total_bytes,
        expires_in_seconds: Some(SNAPSHOT_FILE_RELAY_TTL_SECONDS),
    }
}

fn snapshot_file_relay_grants(
    response: crate::file_relay::CreateFileRelayTransferResponse,
    relay_url: String,
    manifest: SnapshotManifest,
) -> SnapshotFileRelayGrantPair {
    SnapshotFileRelayGrantPair {
        upload: SnapshotFileRelayGrant {
            transfer_id: response.transfer_id.clone(),
            relay_url: relay_url.clone(),
            token: response.upload_token,
            role: SnapshotFileRelayGrantRole::Upload,
            chunk_size_bytes: response.chunk_size_bytes,
            chunk_count: response.chunk_count,
            expires_at: response.expires_at.clone(),
            manifest: manifest.clone(),
        },
        download: SnapshotFileRelayGrant {
            transfer_id: response.transfer_id,
            relay_url,
            token: response.download_token,
            role: SnapshotFileRelayGrantRole::Download,
            chunk_size_bytes: response.chunk_size_bytes,
            chunk_count: response.chunk_count,
            expires_at: response.expires_at,
            manifest,
        },
    }
}
