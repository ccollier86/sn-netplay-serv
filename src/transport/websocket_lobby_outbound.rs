//! Lobby WebSocket outbound helpers.
//!
//! Lobby sockets use a smaller message set than gameplay sockets, so this module
//! owns lobby-specific serialization and error mapping.

use crate::lobbies::LobbyError;
use crate::protocol::LobbyServerMessage;
use axum::extract::ws::{Message, WebSocket};
use futures_util::SinkExt;
use futures_util::stream::SplitSink;

/// Split WebSocket sender used by lobby transport handlers.
pub type LobbySocketSender = SplitSink<WebSocket, Message>;

/// Sends a typed lobby server message as JSON text.
pub async fn send_lobby_server_message(
    sender: &mut LobbySocketSender,
    message: &LobbyServerMessage,
) -> Result<(), axum::Error> {
    let payload = serde_json::to_string(message).expect("lobby message serializes");

    sender.send(Message::Text(payload.into())).await
}

/// Sends a stable lobby protocol error.
pub async fn send_lobby_static_error(
    sender: &mut LobbySocketSender,
    code: &'static str,
    message: &'static str,
) -> Result<(), axum::Error> {
    send_lobby_server_message(sender, &lobby_static_error(code, message)).await
}

/// Sends a lobby-domain error as a stable protocol error.
pub async fn send_lobby_error(
    sender: &mut LobbySocketSender,
    error: LobbyError,
) -> Result<(), axum::Error> {
    send_lobby_server_message(sender, &lobby_error_message(error)).await
}

/// Sends an error on a lobby socket that failed before splitting.
pub async fn send_lobby_upgrade_error(mut socket: WebSocket, error: LobbyError) {
    let payload =
        serde_json::to_string(&lobby_error_message(error)).expect("lobby error serializes");
    let _ = socket.send(Message::Text(payload.into())).await;
    let _ = socket.close().await;
}

fn lobby_static_error(code: &'static str, message: &'static str) -> LobbyServerMessage {
    LobbyServerMessage::Error {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn lobby_error_message(error: LobbyError) -> LobbyServerMessage {
    match error {
        LobbyError::NotFound => lobby_static_error("lobbyNotFound", "Lobby was not found."),
        LobbyError::LobbyFull => lobby_static_error("lobbyFull", "Lobby is full."),
        LobbyError::LobbyClosed => lobby_static_error("lobbyClosed", "Lobby is closed."),
        LobbyError::StaleLobbyEpoch => {
            lobby_static_error("staleLobbyEpoch", "Lobby state changed; refresh and retry.")
        }
        LobbyError::ResumeTokenInvalid => lobby_static_error(
            "lobbyResumeTokenInvalid",
            "Lobby reconnect token is invalid.",
        ),
        LobbyError::PlayerSlotUnavailable => lobby_static_error(
            "lobbyPlayerSlotUnavailable",
            "Lobby player slot is unavailable.",
        ),
        LobbyError::UnknownConnection => lobby_static_error(
            "unknownLobbyConnection",
            "Connection is not assigned to this lobby.",
        ),
        LobbyError::HostOnly => {
            lobby_static_error("lobbyHostOnly", "Only Player 1 can perform this action.")
        }
        LobbyError::StaleGameProposal => lobby_static_error(
            "staleLobbyGameProposal",
            "Selected game changed; refresh and retry.",
        ),
        LobbyError::PlayersNotReady => {
            lobby_static_error("lobbyPlayersNotReady", "Players are not ready yet.")
        }
        LobbyError::RomRelayUnavailable => lobby_static_error(
            "lobbyRomRelayUnavailable",
            "Temporary session access is not available.",
        ),
        LobbyError::RomRelayUnsupported => lobby_static_error(
            "lobbyRomRelayUnsupported",
            "Temporary session access is not supported by every player.",
        ),
        LobbyError::RomRelayTooLarge => lobby_static_error(
            "lobbyRomRelayTooLarge",
            "This game is too large for temporary session access.",
        ),
        LobbyError::InvalidPayload => {
            lobby_static_error("invalidLobbyPayload", "Lobby payload is invalid.")
        }
    }
}
