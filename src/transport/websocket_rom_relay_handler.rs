//! WebSocket handling for direct-invite temporary ROM relay.

use crate::file_relay::{
    CreateFileRelayTransferRequest, FileRelayBrokerError, FileRelayTransferKind,
};
use crate::http::AppServices;
use crate::protocol::RomRelayBlockReason;
use crate::protocol::{RomRelayGrant, RomRelayGrantRole};
use crate::rooms::{ConnectionId, InviteCode, RomRelayGrantPair, RomRelayTransferIntent};
use tracing::warn;

const ROM_FILE_RELAY_TTL_SECONDS: u64 = 600;

/// Creates a direct-invite ROM relay transfer and privately grants host upload.
pub async fn handle_rom_relay_request(
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
) -> Result<(), RomRelayBlockReason> {
    if !services.file_relay_policy.direct_roms_enabled {
        return Err(RomRelayBlockReason::Disabled);
    }
    if !services.file_relay.is_enabled() {
        return Err(RomRelayBlockReason::BrokerUnavailable);
    }
    let relay_url = services
        .file_relay
        .public_base_url()
        .ok_or(RomRelayBlockReason::BrokerUnavailable)?
        .to_string();
    let intent = services
        .rooms
        .prepare_rom_relay(invite_code.clone(), connection_id)
        .await?;
    let response = services
        .file_relay
        .create_transfer(create_rom_relay_request(&intent))
        .await
        .map_err(|error| {
            warn!(error = %error, reason = %broker_failure_reason(&error), "direct ROM file relay transfer creation failed");
            RomRelayBlockReason::BrokerUnavailable
        })?;
    let grants = rom_relay_grants(response, relay_url, &intent);

    services
        .rooms
        .grant_rom_relay_upload(invite_code.clone(), connection_id, grants)
        .await
}

fn create_rom_relay_request(intent: &RomRelayTransferIntent) -> CreateFileRelayTransferRequest {
    let content_hash = intent.rom.normalized_hash();
    CreateFileRelayTransferRequest {
        room_id: intent.room_id.to_string(),
        sender_player_id: format!("player-{}", intent.sender_player_index.display_number()),
        receiver_player_id: format!("player-{}", intent.receiver_player_index.display_number()),
        kind: FileRelayTransferKind::Rom,
        sha256: content_hash.clone(),
        size_bytes: intent.rom.size_bytes,
        expires_in_seconds: Some(ROM_FILE_RELAY_TTL_SECONDS),
        room_epoch: Some(intent.room_epoch),
        session_epoch: Some(intent.session_epoch),
        system: Some(intent.rom.system.clone()),
        core_id: Some(intent.rom.core_id.clone()),
        content_hash: Some(content_hash),
        file_name: intent.rom.file_name.clone(),
        extension: intent.rom.extension.clone(),
        display_name: Some(intent.rom.display_name.clone()),
        single_use: true,
    }
}

fn rom_relay_grants(
    response: crate::file_relay::CreateFileRelayTransferResponse,
    relay_url: String,
    intent: &RomRelayTransferIntent,
) -> RomRelayGrantPair {
    RomRelayGrantPair {
        upload: RomRelayGrant {
            transfer_id: response.transfer_id.clone(),
            relay_url: relay_url.clone(),
            token: response.upload_token,
            role: RomRelayGrantRole::Upload,
            rom: intent.rom.clone(),
            sender_player_index: intent.sender_player_index.zero_based(),
            receiver_player_index: intent.receiver_player_index.zero_based(),
            chunk_size_bytes: response.chunk_size_bytes,
            chunk_count: response.chunk_count,
            expires_at: response.expires_at.clone(),
        },
        download: RomRelayGrant {
            transfer_id: response.transfer_id,
            relay_url,
            token: response.download_token,
            role: RomRelayGrantRole::Download,
            rom: intent.rom.clone(),
            sender_player_index: intent.sender_player_index.zero_based(),
            receiver_player_index: intent.receiver_player_index.zero_based(),
            chunk_size_bytes: response.chunk_size_bytes,
            chunk_count: response.chunk_count,
            expires_at: response.expires_at,
        },
    }
}

fn broker_failure_reason(error: &FileRelayBrokerError) -> &'static str {
    match error {
        FileRelayBrokerError::Disabled => "disabled",
        FileRelayBrokerError::InvalidUrl => "invalid-url",
        FileRelayBrokerError::RequestFailed => "request-failed",
        FileRelayBrokerError::RequestTimedOut => "request-timeout",
        FileRelayBrokerError::UnexpectedStatus(_) => "unexpected-status",
        FileRelayBrokerError::InvalidResponse => "invalid-response",
    }
}
