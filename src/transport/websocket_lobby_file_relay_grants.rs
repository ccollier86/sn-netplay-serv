//! Shared lobby file-relay grant construction.
//!
//! This module maps trusted file-relay broker responses into private lobby
//! grants. It must not validate lobby permissions, call the broker, or send
//! WebSocket messages.

use crate::file_relay::CreateFileRelayTransferResponse;
use crate::protocol::{
    LobbyFileRelayGrant, LobbyFileRelayGrantPair, LobbyFileRelayGrantRole,
    LobbyFileRelayMaterialKind, LobbyStartupStateTransferMetadata,
};
use crate::rooms::PlayerIndex;
use uuid::Uuid;

/// Input needed to create a private lobby file-relay grant pair.
pub struct LobbyFileRelayGrantPairRequest {
    /// Broker-created transfer response.
    pub response: CreateFileRelayTransferResponse,
    /// Public file relay base URL.
    pub relay_url: String,
    /// Material kind represented by the transfer.
    pub material_kind: LobbyFileRelayMaterialKind,
    /// Selected lobby proposal id.
    pub proposal_id: Uuid,
    /// Sender player slot.
    pub sender_player_index: PlayerIndex,
    /// Receiver player slot.
    pub receiver_player_index: PlayerIndex,
    /// Expected payload SHA-256.
    pub sha256: String,
    /// Expected payload byte size.
    pub size_bytes: u64,
    /// Startup-state metadata for state transfers.
    pub startup_state: Option<LobbyStartupStateTransferMetadata>,
}

/// Creates private upload/download grants for a lobby file-relay transfer.
pub fn lobby_file_relay_grants(request: LobbyFileRelayGrantPairRequest) -> LobbyFileRelayGrantPair {
    LobbyFileRelayGrantPair {
        upload: LobbyFileRelayGrant {
            transfer_id: request.response.transfer_id.clone(),
            relay_url: request.relay_url.clone(),
            token: request.response.upload_token,
            role: LobbyFileRelayGrantRole::Upload,
            material_kind: request.material_kind,
            proposal_id: request.proposal_id,
            sender_player_index: request.sender_player_index.zero_based(),
            receiver_player_index: request.receiver_player_index.zero_based(),
            sha256: request.sha256.clone(),
            size_bytes: request.size_bytes,
            chunk_size_bytes: request.response.chunk_size_bytes,
            chunk_count: request.response.chunk_count,
            expires_at: request.response.expires_at.clone(),
            startup_state: request.startup_state.clone(),
        },
        download: LobbyFileRelayGrant {
            transfer_id: request.response.transfer_id,
            relay_url: request.relay_url,
            token: request.response.download_token,
            role: LobbyFileRelayGrantRole::Download,
            material_kind: request.material_kind,
            proposal_id: request.proposal_id,
            sender_player_index: request.sender_player_index.zero_based(),
            receiver_player_index: request.receiver_player_index.zero_based(),
            sha256: request.sha256,
            size_bytes: request.size_bytes,
            chunk_size_bytes: request.response.chunk_size_bytes,
            chunk_count: request.response.chunk_count,
            expires_at: request.response.expires_at,
            startup_state: request.startup_state,
        },
    }
}
