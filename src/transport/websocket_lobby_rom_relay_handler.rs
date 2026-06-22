//! WebSocket handling for lobby temporary ROM relay requests.
//!
//! This handler mirrors large snapshot relay flow: validate the lobby intent,
//! ask the trusted file-relay service for scoped tokens, then send private
//! grants to only the upload and download sockets.

use crate::file_relay::{
    CreateFileRelayTransferRequest, FileRelayBrokerError, FileRelayTransferKind,
};
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
        record_lobby_rom_relay_failure(
            services,
            invite_code,
            "temporary ROM relay unavailable reason=disabled".to_string(),
        )
        .await;
        return Err(LobbyError::RomRelayUnavailable);
    }

    let Some(relay_url) = services.file_relay.public_base_url() else {
        record_lobby_rom_relay_failure(
            services,
            invite_code,
            "temporary ROM relay unavailable reason=missing-public-url".to_string(),
        )
        .await;
        return Err(LobbyError::RomRelayUnavailable);
    };
    let relay_url = relay_url.to_string();
    let intent = match services
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
        .await
    {
        Ok(intent) => intent,
        Err(error) => {
            record_lobby_rom_relay_failure(
                services,
                invite_code,
                rom_relay_request_failure_detail(&error).to_string(),
            )
            .await;
            return Err(error);
        }
    };
    let response = match services
        .file_relay
        .create_transfer(create_rom_relay_request(&intent))
        .await
    {
        Ok(response) => response,
        Err(error) => {
            let broker_reason = rom_relay_broker_failure_reason(&error);
            warn!(error = %error, reason = %broker_reason, "lobby ROM file relay transfer creation failed");
            record_lobby_rom_relay_failure(
                services,
                invite_code,
                format!(
                    "temporary ROM relay broker failed p{}->p{} bytes={} reason={}",
                    intent.sender_player_index.display_number(),
                    intent.receiver_player_index.display_number(),
                    intent.size_bytes,
                    broker_reason
                ),
            )
            .await;
            return Err(LobbyError::RomRelayUnavailable);
        }
    };
    let grants = rom_relay_grants(response, relay_url, &intent);

    match services
        .lobbies
        .grant_lobby_rom_relay_transfer(invite_code.clone(), intent, grants)
        .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            record_lobby_rom_relay_failure(
                services,
                invite_code,
                rom_relay_request_failure_detail(&error).to_string(),
            )
            .await;
            Err(error)
        }
    }
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
        room_epoch: None,
        session_epoch: None,
        system: Some(intent.game.system_id.clone()),
        core_id: Some(intent.game.core_id.clone()),
        content_hash: Some(intent.sha256.clone()),
        file_name: None,
        extension: None,
        display_name: Some(intent.game.title.clone()),
        single_use: true,
    }
}

async fn record_lobby_rom_relay_failure(
    services: &AppServices,
    invite_code: &InviteCode,
    detail: String,
) {
    let _ = services
        .lobbies
        .record_lobby_diagnostic(invite_code.clone(), "lobbyRomRelayFailed", detail)
        .await;
}

fn rom_relay_request_failure_detail(error: &LobbyError) -> &'static str {
    match error {
        LobbyError::HostOnly => "temporary ROM relay rejected reason=host-only",
        LobbyError::InvalidPayload => "temporary ROM relay rejected reason=invalid-payload",
        LobbyError::PlayerSlotUnavailable => {
            "temporary ROM relay rejected reason=player-slot-unavailable"
        }
        LobbyError::RomRelayTooLarge => "temporary ROM relay rejected reason=too-large",
        LobbyError::RomRelayUnavailable => "temporary ROM relay unavailable reason=unavailable",
        LobbyError::RomRelayUnsupported => "temporary ROM relay rejected reason=unsupported",
        LobbyError::StaleGameProposal => "temporary ROM relay rejected reason=stale-game",
        LobbyError::StaleLobbyEpoch => "temporary ROM relay rejected reason=stale-lobby",
        LobbyError::UnknownConnection => "temporary ROM relay rejected reason=unknown-connection",
        _ => "temporary ROM relay rejected reason=lobby-error",
    }
}

fn rom_relay_broker_failure_reason(error: &FileRelayBrokerError) -> String {
    match error {
        FileRelayBrokerError::Disabled => "disabled".to_string(),
        FileRelayBrokerError::InvalidUrl => "invalid-url".to_string(),
        FileRelayBrokerError::RequestFailed => "request-failed".to_string(),
        FileRelayBrokerError::RequestTimedOut => "request-timeout".to_string(),
        FileRelayBrokerError::UnexpectedStatus(status) => format!("status-{status}"),
        FileRelayBrokerError::InvalidResponse => "invalid-response".to_string(),
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
