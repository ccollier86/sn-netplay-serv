//! WebSocket handling for lobby selected startup-state relay requests.
//!
//! This handler validates a lobby startup-state intent, asks the trusted
//! file-relay service for scoped save-state tokens, then sends private grants
//! only to the upload and download sockets.

use crate::file_relay::{
    CreateFileRelayTransferRequest, FileRelayBrokerError, FileRelayTransferKind,
};
use crate::http::AppServices;
use crate::lobbies::{LobbyError, LobbyStartupStateRelayLimits};
use crate::protocol::{LobbyFileRelayMaterialKind, LobbyStartupStateTransferMetadata};
use crate::rooms::{ConnectionId, InviteCode, PlayerIndex};
use crate::transport::websocket_lobby_file_relay_grants::{
    LobbyFileRelayGrantPairRequest, lobby_file_relay_grants,
};
use tracing::warn;
use uuid::Uuid;

const STARTUP_STATE_FILE_RELAY_TTL_SECONDS: u64 = 600;

/// Creates private file-relay grants for a host-to-player startup-state transfer.
pub async fn handle_lobby_startup_state_relay_request(
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    proposal_id: Uuid,
    receiver_player_index: PlayerIndex,
    state: LobbyStartupStateTransferMetadata,
) -> Result<(), LobbyError> {
    if !services
        .file_relay_policy
        .can_relay_save_states(services.file_relay.as_ref())
    {
        record_lobby_startup_state_relay_failure(
            services,
            invite_code,
            "startup state relay unavailable reason=disabled".to_string(),
        )
        .await;
        return Err(LobbyError::StartupStateRelayUnavailable);
    }

    let Some(relay_url) = services.file_relay.public_base_url() else {
        record_lobby_startup_state_relay_failure(
            services,
            invite_code,
            "startup state relay unavailable reason=missing-public-url".to_string(),
        )
        .await;
        return Err(LobbyError::StartupStateRelayUnavailable);
    };
    let relay_url = relay_url.to_string();
    let intent = match services
        .lobbies
        .prepare_lobby_startup_state_relay_transfer(
            invite_code.clone(),
            connection_id,
            proposal_id,
            receiver_player_index,
            state,
            LobbyStartupStateRelayLimits {
                max_bytes: crate::limits::MAX_SNAPSHOT_BYTES,
            },
        )
        .await
    {
        Ok(intent) => intent,
        Err(error) => {
            record_lobby_startup_state_relay_failure(
                services,
                invite_code,
                startup_state_relay_request_failure_detail(&error).to_string(),
            )
            .await;
            return Err(error);
        }
    };
    let response = match services
        .file_relay
        .create_transfer(create_startup_state_relay_request(&intent))
        .await
    {
        Ok(response) => response,
        Err(error) => {
            let broker_reason = startup_state_relay_broker_failure_reason(&error);
            warn!(error = %error, reason = %broker_reason, "lobby startup-state file relay transfer creation failed");
            record_lobby_startup_state_relay_failure(
                services,
                invite_code,
                format!(
                    "startup state relay broker failed p{}->p{} bytes={} reason={}",
                    intent.sender_player_index.display_number(),
                    intent.receiver_player_index.display_number(),
                    intent.state.size_bytes,
                    broker_reason
                ),
            )
            .await;
            return Err(LobbyError::StartupStateRelayUnavailable);
        }
    };
    let grants = lobby_file_relay_grants(LobbyFileRelayGrantPairRequest {
        response,
        relay_url,
        material_kind: LobbyFileRelayMaterialKind::StartupState,
        proposal_id: intent.proposal_id,
        sender_player_index: intent.sender_player_index,
        receiver_player_index: intent.receiver_player_index,
        sha256: intent.state.sha256.clone(),
        size_bytes: intent.state.size_bytes,
        startup_state: Some(intent.state.clone()),
    });

    match services
        .lobbies
        .grant_lobby_startup_state_relay_transfer(invite_code.clone(), intent, grants)
        .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            record_lobby_startup_state_relay_failure(
                services,
                invite_code,
                startup_state_relay_request_failure_detail(&error).to_string(),
            )
            .await;
            Err(error)
        }
    }
}

fn create_startup_state_relay_request(
    intent: &crate::lobbies::LobbyStartupStateRelayTransferIntent,
) -> CreateFileRelayTransferRequest {
    CreateFileRelayTransferRequest {
        room_id: intent.lobby_id.to_string(),
        sender_player_id: format!("player-{}", intent.sender_player_index.display_number()),
        receiver_player_id: format!("player-{}", intent.receiver_player_index.display_number()),
        kind: FileRelayTransferKind::SaveState,
        sha256: intent.state.sha256.clone(),
        size_bytes: intent.state.size_bytes,
        expires_in_seconds: Some(STARTUP_STATE_FILE_RELAY_TTL_SECONDS),
        room_epoch: None,
        session_epoch: None,
        system: Some(intent.game.system_id.clone()),
        core_id: Some(intent.game.core_id.clone()),
        content_hash: intent.game.content_sha256.clone(),
        file_name: None,
        extension: Some("state".to_string()),
        display_name: intent
            .state
            .label
            .clone()
            .or_else(|| intent.game.start_state_label.clone()),
        single_use: true,
    }
}

async fn record_lobby_startup_state_relay_failure(
    services: &AppServices,
    invite_code: &InviteCode,
    detail: String,
) {
    let _ = services
        .lobbies
        .record_lobby_diagnostic(invite_code.clone(), "lobbyStartupStateRelayFailed", detail)
        .await;
}

fn startup_state_relay_request_failure_detail(error: &LobbyError) -> &'static str {
    match error {
        LobbyError::HostOnly => "startup state relay rejected reason=host-only",
        LobbyError::InvalidPayload => "startup state relay rejected reason=invalid-payload",
        LobbyError::PlayerSlotUnavailable => {
            "startup state relay rejected reason=player-slot-unavailable"
        }
        LobbyError::StartupStateRelayTooLarge => "startup state relay rejected reason=too-large",
        LobbyError::StartupStateRelayUnavailable => {
            "startup state relay unavailable reason=unavailable"
        }
        LobbyError::StartupStateRelayUnsupported => {
            "startup state relay rejected reason=unsupported"
        }
        LobbyError::StaleGameProposal => "startup state relay rejected reason=stale-game",
        LobbyError::StaleLobbyEpoch => "startup state relay rejected reason=stale-lobby",
        LobbyError::UnknownConnection => "startup state relay rejected reason=unknown-connection",
        _ => "startup state relay rejected reason=lobby-error",
    }
}

fn startup_state_relay_broker_failure_reason(error: &FileRelayBrokerError) -> String {
    match error {
        FileRelayBrokerError::Disabled => "disabled".to_string(),
        FileRelayBrokerError::InvalidUrl => "invalid-url".to_string(),
        FileRelayBrokerError::RequestFailed => "request-failed".to_string(),
        FileRelayBrokerError::RequestTimedOut => "request-timeout".to_string(),
        FileRelayBrokerError::UnexpectedStatus(status) => format!("status-{status}"),
        FileRelayBrokerError::InvalidResponse => "invalid-response".to_string(),
    }
}
