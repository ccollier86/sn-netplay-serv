//! Outbound WebSocket message helpers.
//!
//! This module owns serialization and stable error-message mapping for socket
//! responses. It does not mutate room state.

use crate::protocol::ServerMessage;
use crate::rooms::RoomError;
use axum::extract::ws::{Message, WebSocket};
use bytes::Bytes;
use futures_util::SinkExt;
use futures_util::stream::SplitSink;

/// Split WebSocket sender used by transport handlers.
pub type SocketSender = SplitSink<WebSocket, Message>;

/// Sends a typed server message as JSON text.
pub async fn send_server_message(
    sender: &mut SocketSender,
    message: &ServerMessage,
) -> Result<(), axum::Error> {
    let payload = serde_json::to_string(message).expect("server message serializes");

    sender.send(Message::Text(payload.into())).await
}

/// Sends a binary WebSocket payload.
pub async fn send_binary_message(
    sender: &mut SocketSender,
    payload: Vec<u8>,
) -> Result<(), axum::Error> {
    sender.send(Message::Binary(payload.into())).await
}

/// Sends a binary WebSocket payload already held as shared bytes.
pub async fn send_binary_bytes(
    sender: &mut SocketSender,
    payload: Bytes,
) -> Result<(), axum::Error> {
    sender.send(Message::Binary(payload)).await
}

/// Sends a stable protocol error.
pub async fn send_static_error(
    sender: &mut SocketSender,
    code: &'static str,
    message: &'static str,
) -> Result<(), axum::Error> {
    send_server_message(sender, &static_error(code, message)).await
}

/// Sends a room-domain error as a stable protocol error.
pub async fn send_room_error(
    sender: &mut SocketSender,
    error: RoomError,
) -> Result<(), axum::Error> {
    send_server_message(sender, &room_error_message(error)).await
}

/// Sends an error on a socket that failed before splitting.
pub async fn send_upgrade_error(mut socket: WebSocket, error: RoomError) {
    let payload = serde_json::to_string(&room_error_message(error)).expect("error serializes");
    let _ = socket.send(Message::Text(payload.into())).await;
    let _ = socket.close().await;
}

fn static_error(code: &'static str, message: &'static str) -> ServerMessage {
    ServerMessage::Error {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn room_error_message(error: RoomError) -> ServerMessage {
    match error {
        RoomError::NotFound => static_error("roomNotFound", "Room was not found."),
        RoomError::RoomFull => static_error("roomFull", "Room is full."),
        RoomError::RoomClosed => static_error("roomClosed", "Room is closed."),
        RoomError::HostSubjectMismatch => {
            static_error("hostMismatch", "This install is not the room host.")
        }
        RoomError::InvalidInviteCode => {
            static_error("invalidInviteCode", "Invite code is invalid.")
        }
        RoomError::RoomNotReady => {
            static_error("roomNotReady", "Room is not ready for this operation.")
        }
        RoomError::HostOnly => {
            static_error("hostOnly", "Only the host can perform this operation.")
        }
        RoomError::SnapshotInvalid => {
            static_error("snapshotInvalid", "Snapshot payload is invalid.")
        }
        RoomError::SnapshotFileRelayUnavailable => static_error(
            "snapshotFileRelayUnavailable",
            "Snapshot file relay is unavailable for this room.",
        ),
        RoomError::LinkPacketInvalid => {
            static_error("linkPacketInvalid", "Link-cable packet is invalid.")
        }
        RoomError::InvalidPayload => static_error("invalidPayload", "Payload is invalid."),
        RoomError::OutOfOrderLinkPacket => {
            static_error("outOfOrderLinkPacket", "Link-cable packet is out of order.")
        }
        RoomError::SlotSpoofing(_) => {
            static_error("slotSpoofing", "Input was sent for the wrong player slot.")
        }
        RoomError::OutOfOrderFrame => {
            static_error("outOfOrderFrame", "Input frame is out of order.")
        }
        RoomError::FutureFrameTooLarge => {
            static_error("futureFrameTooLarge", "Input frame is too far ahead.")
        }
        RoomError::NotPlaying => static_error("notPlaying", "Room is not playing."),
        RoomError::UnknownConnection => static_error(
            "unknownConnection",
            "Connection is not assigned to this room.",
        ),
        RoomError::CompatibilityMismatch => static_error(
            "compatibilityMismatch",
            "Netplay compatibility does not match.",
        ),
        RoomError::StaleRoomEpoch => {
            static_error("staleRoomEpoch", "Room state changed; refresh and retry.")
        }
        RoomError::StaleSessionEpoch => static_error(
            "staleSessionEpoch",
            "Netplay session changed; refresh and retry.",
        ),
        RoomError::ResumeTokenInvalid => {
            static_error("resumeTokenInvalid", "Reconnect token is invalid.")
        }
        RoomError::RecoveryExpired => {
            static_error("recoveryExpired", "Reconnect recovery window expired.")
        }
        RoomError::VoiceUnavailable => {
            static_error("voiceUnavailable", "Voice chat is unavailable.")
        }
    }
}
