//! WebSocket handling for lobby temporary ROM relay requests.
//!
//! This handler mirrors large snapshot relay flow: validate the lobby intent,
//! ask the trusted file-relay service for scoped tokens, then send private
//! grants to only the upload and download sockets.

use crate::file_relay::{CreateFileRelayTransferRequest, FileRelayTransferKind};
use crate::http::AppServices;
use crate::lobbies::{LobbyError, LobbyRomRelayLimits, LobbyRomRelayTransferIntent};
use crate::protocol::{LobbyFileRelayGrant, LobbyFileRelayGrantPair, LobbyFileRelayGrantRole};
use crate::rooms::{ConnectionId, InviteCode, PlayerIndex};
use tracing::warn;
use uuid::Uuid;

const ROM_FILE_RELAY_TTL_SECONDS: u64 = 600;

/// Creates private file-relay grants for a host-to-player ROM transfer.
pub async fn handle_lobby_rom_relay_request(
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    proposal_id: Uuid,
    receiver_player_index: PlayerIndex,
) -> Result<(), LobbyError> {
    if !services
        .file_relay_policy
        .can_relay_temporary_roms(services.file_relay.as_ref())
    {
        return Err(LobbyError::RomRelayUnavailable);
    }

    let relay_url = services
        .file_relay
        .public_base_url()
        .ok_or(LobbyError::RomRelayUnavailable)?
        .to_string();
    let intent = services
        .lobbies
        .prepare_lobby_rom_relay_transfer(
            invite_code.clone(),
            connection_id,
            proposal_id,
            receiver_player_index,
            LobbyRomRelayLimits {
                max_bytes: services.file_relay_policy.temporary_rom_max_bytes,
            },
        )
        .await?;
    let response = services
        .file_relay
        .create_transfer(create_rom_relay_request(&intent))
        .await
        .map_err(|error| {
            warn!(error = %error, "lobby ROM file relay transfer creation failed");
            LobbyError::RomRelayUnavailable
        })?;
    let grants = rom_relay_grants(response, relay_url, &intent);

    services
        .lobbies
        .grant_lobby_rom_relay_transfer(invite_code.clone(), intent, grants)
        .await
}

fn create_rom_relay_request(
    intent: &LobbyRomRelayTransferIntent,
) -> CreateFileRelayTransferRequest {
    CreateFileRelayTransferRequest {
        room_id: intent.lobby_id.to_string(),
        sender_player_id: format!("player-{}", intent.sender_player_index.display_number()),
        receiver_player_id: format!("player-{}", intent.receiver_player_index.display_number()),
        kind: FileRelayTransferKind::Rom,
        sha256: intent.sha256.clone(),
        size_bytes: intent.size_bytes,
        expires_in_seconds: Some(ROM_FILE_RELAY_TTL_SECONDS),
    }
}

fn rom_relay_grants(
    response: crate::file_relay::CreateFileRelayTransferResponse,
    relay_url: String,
    intent: &LobbyRomRelayTransferIntent,
) -> LobbyFileRelayGrantPair {
    LobbyFileRelayGrantPair {
        upload: LobbyFileRelayGrant {
            transfer_id: response.transfer_id.clone(),
            relay_url: relay_url.clone(),
            token: response.upload_token,
            role: LobbyFileRelayGrantRole::Upload,
            proposal_id: intent.proposal_id,
            sender_player_index: intent.sender_player_index.zero_based(),
            receiver_player_index: intent.receiver_player_index.zero_based(),
            sha256: intent.sha256.clone(),
            size_bytes: intent.size_bytes,
            chunk_size_bytes: response.chunk_size_bytes,
            chunk_count: response.chunk_count,
            expires_at: response.expires_at.clone(),
        },
        download: LobbyFileRelayGrant {
            transfer_id: response.transfer_id,
            relay_url,
            token: response.download_token,
            role: LobbyFileRelayGrantRole::Download,
            proposal_id: intent.proposal_id,
            sender_player_index: intent.sender_player_index.zero_based(),
            receiver_player_index: intent.receiver_player_index.zero_based(),
            sha256: intent.sha256.clone(),
            size_bytes: intent.size_bytes,
            chunk_size_bytes: response.chunk_size_bytes,
            chunk_count: response.chunk_count,
            expires_at: response.expires_at,
        },
    }
}
