//! Lobby WebSocket wire messages.
//!
//! These messages are separate from gameplay room messages so lobbies can evolve
//! without affecting Android's existing direct room path.

use crate::lobbies::{
    LobbyActivityKind, LobbyChatMessageView, LobbyGameCandidate, LobbyGameReadinessStatus,
    LobbyView,
};
use crate::protocol::{LobbyFileRelayGrant, LobbyStartupStateTransferMetadata};
use crate::rooms::PlayerVoiceJoinGrant;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client-to-relay lobby WebSocket message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LobbyClientMessage {
    /// Lightweight keepalive.
    Ping,
    /// Host selects or replaces the proposed game.
    SelectGame {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Proposed game details.
        game: LobbyGameCandidate,
    },
    /// Client reports whether it can launch the selected game.
    SetGameReadiness {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal being evaluated.
        proposal_id: Uuid,
        /// Readiness status for the local player.
        status: LobbyGameReadinessStatus,
        /// Optional short reason shown in UI.
        detail: Option<String>,
    },
    /// Host requests that all ready clients launch the selected game.
    LaunchGame {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal to launch.
        proposal_id: Uuid,
    },
    /// Host asks the relay to prepare a temporary ROM transfer for one player.
    RequestRomTransfer {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal being prepared.
        proposal_id: Uuid,
        /// Zero-based receiver player index.
        receiver_player_index: u8,
    },
    /// Host asks the relay to prepare a selected startup-state transfer.
    RequestStartupStateTransfer {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal being prepared.
        proposal_id: Uuid,
        /// Zero-based receiver player index.
        receiver_player_index: u8,
        /// Sender-side startup-state material metadata.
        state: LobbyStartupStateTransferMetadata,
    },
    /// Host publishes the direct gameplay room once it is ready.
    PublishGameRoom {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal being launched.
        proposal_id: Uuid,
        /// User-facing gameplay room invite code.
        room_invite_code: String,
    },
    /// Client reports that the launched child game has ended.
    ReturnToLobby {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Selected game proposal that was active.
        proposal_id: Uuid,
    },
    /// Sends a lobby-scoped chat message.
    Chat {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Chat body.
        body: String,
    },
    /// Requests a fresh private token for the lobby voice room.
    RefreshVoiceToken {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
    },
    /// Reports meaningful activity that should retain the lobby.
    ReportActivity {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Activity type safe for telemetry.
        kind: LobbyActivityKind,
    },
    /// Client intentionally leaves the lobby.
    Leave {
        /// Lobby epoch observed by the client.
        lobby_epoch: u64,
        /// Optional safe reason string for future diagnostics.
        reason: Option<String>,
    },
}

/// Relay-to-client lobby WebSocket message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LobbyServerMessage {
    /// Reply to client ping.
    Pong,
    /// Initial socket join grant.
    LobbyJoined {
        /// Current lobby event sequence.
        event_seq: u64,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Assigned zero-based player index.
        your_player_index: u8,
        /// Opaque token for reclaiming this lobby slot.
        resume_token: String,
        /// Current lobby state.
        lobby: LobbyView,
        /// Optional player-specific voice grant.
        #[serde(skip_serializing_if = "Option::is_none")]
        voice: Option<PlayerVoiceJoinGrant>,
    },
    /// Lobby state changed.
    LobbyStateChanged {
        /// Current lobby event sequence.
        event_seq: u64,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Current lobby state.
        lobby: LobbyView,
    },
    /// Lobby chat message.
    ChatMessage {
        /// Chat details.
        message: LobbyChatMessageView,
    },
    /// Private upload grant for the host.
    RomTransferUploadGranted {
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Private file relay grant.
        grant: LobbyFileRelayGrant,
    },
    /// Private download grant for the receiver.
    RomTransferDownloadReady {
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Private file relay grant.
        grant: LobbyFileRelayGrant,
    },
    /// Private startup-state upload grant for the host.
    StartupStateTransferUploadGranted {
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Private file relay grant.
        grant: LobbyFileRelayGrant,
    },
    /// Private startup-state download grant for the receiver.
    StartupStateTransferDownloadReady {
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Private file relay grant.
        grant: LobbyFileRelayGrant,
    },
    /// Private refreshed voice token for this lobby socket.
    VoiceTokenRefreshed {
        /// Current lobby event sequence.
        event_seq: u64,
        /// Current lobby epoch.
        lobby_epoch: u64,
        /// Fresh private voice grant.
        voice: PlayerVoiceJoinGrant,
    },
    /// Lobby was closed by the server.
    LobbyClosed {
        /// Final lobby event sequence.
        event_seq: u64,
        /// Final lobby epoch.
        lobby_epoch: u64,
        /// Safe close reason.
        reason: String,
        /// Final lobby state.
        lobby: LobbyView,
    },
    /// Stable lobby protocol error.
    Error {
        /// Machine-readable error code.
        code: String,
        /// Safe user-facing message.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lobbies::{LobbyServerCapabilities, LobbyStatus, LobbyVisibility};
    use crate::protocol::{
        LobbyFileRelayGrantRole, LobbyFileRelayMaterialKind, LobbyStartupStateRestorePolicy,
    };
    use crate::rooms::RoomId;
    use serde_json::json;

    #[test]
    fn lobby_client_messages_accept_desktop_camel_case_fields() {
        let message = serde_json::from_value::<LobbyClientMessage>(json!({
            "type": "chat",
            "lobbyEpoch": 3,
            "body": "hello"
        }))
        .expect("chat message");

        assert!(matches!(
            message,
            LobbyClientMessage::Chat {
                lobby_epoch: 3,
                body
            } if body == "hello"
        ));
    }

    #[test]
    fn lobby_server_messages_emit_desktop_camel_case_fields() {
        let message = LobbyServerMessage::LobbyJoined {
            event_seq: 4,
            lobby_epoch: 2,
            your_player_index: 0,
            resume_token: "resume-token".to_string(),
            voice: None,
            lobby: LobbyView {
                lobby_id: RoomId::new(),
                event_seq: 4,
                lobby_epoch: 2,
                invite_code: "AB23-CD".to_string(),
                created_at_ms: 1,
                updated_at_ms: 2,
                last_meaningful_activity_at_ms: 2,
                status: LobbyStatus::Open,
                visibility: LobbyVisibility::Private,
                capabilities: LobbyServerCapabilities::current(4, true, true),
                players: Vec::new(),
                selected_game: None,
                game_readiness: Vec::new(),
                pending_launch: None,
                voice: None,
            },
        };

        let payload = serde_json::to_value(message).expect("server message");

        assert_eq!(payload["eventSeq"], 4);
        assert_eq!(payload["lobbyEpoch"], 2);
        assert_eq!(payload["yourPlayerIndex"], 0);
        assert!(payload.get("event_seq").is_none());
        assert!(payload.get("lobby_epoch").is_none());
        assert!(payload.get("your_player_index").is_none());
    }

    #[test]
    fn lobby_rom_transfer_grant_keeps_legacy_wire_shape() {
        let grant = LobbyFileRelayGrant {
            transfer_id: "rom-transfer-1".to_owned(),
            relay_url: "https://relay.test".to_owned(),
            token: "token".to_owned(),
            role: LobbyFileRelayGrantRole::Download,
            material_kind: LobbyFileRelayMaterialKind::Game,
            proposal_id: uuid::Uuid::new_v4(),
            sender_player_index: 0,
            receiver_player_index: 1,
            sha256: "a".repeat(64),
            size_bytes: 128,
            chunk_size_bytes: 128,
            chunk_count: 1,
            expires_at: "2026-06-22T12:00:00Z".to_owned(),
            startup_state: None,
        };

        let payload = serde_json::to_value(LobbyServerMessage::RomTransferDownloadReady {
            lobby_epoch: 5,
            grant,
        })
        .expect("server message");

        assert_eq!(payload["type"], "romTransferDownloadReady");
        assert!(payload["grant"].get("materialKind").is_none());
        assert!(payload["grant"].get("startupState").is_none());
    }

    #[test]
    fn lobby_startup_state_transfer_grant_names_material_kind() {
        let grant = LobbyFileRelayGrant {
            transfer_id: "state-transfer-1".to_owned(),
            relay_url: "https://relay.test".to_owned(),
            token: "token".to_owned(),
            role: LobbyFileRelayGrantRole::Download,
            material_kind: LobbyFileRelayMaterialKind::StartupState,
            proposal_id: uuid::Uuid::new_v4(),
            sender_player_index: 0,
            receiver_player_index: 1,
            sha256: "b".repeat(64),
            size_bytes: 128,
            chunk_size_bytes: 128,
            chunk_count: 1,
            expires_at: "2026-06-22T12:00:00Z".to_owned(),
            startup_state: Some(LobbyStartupStateTransferMetadata {
                sha256: "b".repeat(64),
                size_bytes: 128,
                label: Some("Autosave".to_owned()),
                restore_policy: LobbyStartupStateRestorePolicy::AfterFrames { frames: 30 },
                state_format: Some("libretro-state".to_owned()),
            }),
        };

        let payload = serde_json::to_value(LobbyServerMessage::StartupStateTransferDownloadReady {
            lobby_epoch: 5,
            grant,
        })
        .expect("server message");

        assert_eq!(payload["type"], "startupStateTransferDownloadReady");
        assert_eq!(payload["grant"]["materialKind"], "startupState");
        assert_eq!(payload["grant"]["startupState"]["label"], "Autosave");
    }
}
