//! Incoming WebSocket message handling.
//!
//! This module converts client messages into room-registry operations. It keeps
//! gameplay protocol decisions separate from the socket event loop.

use crate::http::AppServices;
use crate::limits::MAX_WEBSOCKET_MESSAGE_BYTES;
use crate::protocol::{ClientMessage, ServerMessage};
use crate::rooms::{ConnectionId, InviteCode, RoomError};
use crate::transport::websocket_outbound::{
    SocketSender, send_room_error, send_server_message, send_static_error,
};

/// Parses and applies one client text message.
pub async fn handle_client_text(
    sender: &mut SocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    payload: &str,
) -> bool {
    if payload.len() > MAX_WEBSOCKET_MESSAGE_BYTES {
        return send_static_error(sender, "messageTooLarge", "Message is too large.")
            .await
            .is_ok();
    }

    match serde_json::from_str::<ClientMessage>(payload) {
        Ok(message) => handle_client_message(sender, services, invite_code, connection_id, message)
            .await
            .is_ok(),
        Err(_) => send_static_error(sender, "invalidMessage", "Message JSON is invalid.")
            .await
            .is_ok(),
    }
}

async fn handle_client_message(
    sender: &mut SocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    message: ClientMessage,
) -> Result<(), axum::Error> {
    match message {
        ClientMessage::Ping => send_server_message(sender, &ServerMessage::Pong).await,
        ClientMessage::SetCompatibilityFingerprint { fingerprint } => {
            apply_room_result(
                sender,
                services
                    .rooms
                    .set_compatibility(invite_code.clone(), connection_id, fingerprint)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        ClientMessage::SetLinkCableCompatibility { compatibility } => {
            apply_room_result(
                sender,
                services
                    .rooms
                    .set_link_cable_compatibility(invite_code.clone(), connection_id, compatibility)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        ClientMessage::Ready => {
            apply_room_result(
                sender,
                services
                    .rooms
                    .mark_ready(invite_code.clone(), connection_id)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        ClientMessage::SnapshotChunk { chunk } => {
            apply_room_result(
                sender,
                services
                    .rooms
                    .relay_snapshot_chunk(invite_code.clone(), connection_id, chunk)
                    .await,
            )
            .await
        }
        ClientMessage::SnapshotComplete { manifest } => {
            apply_room_result(
                sender,
                services
                    .rooms
                    .relay_snapshot_complete(invite_code.clone(), connection_id, manifest)
                    .await,
            )
            .await
        }
        ClientMessage::InputFrame { input } => {
            apply_room_result(
                sender,
                services
                    .rooms
                    .relay_input_frame(invite_code.clone(), connection_id, input)
                    .await,
            )
            .await
        }
        ClientMessage::LinkCablePacket { packet } => {
            apply_room_result(
                sender,
                services
                    .rooms
                    .relay_link_cable_packet(invite_code.clone(), connection_id, packet)
                    .await,
            )
            .await
        }
    }
}

async fn apply_room_result(
    sender: &mut SocketSender,
    result: Result<(), RoomError>,
) -> Result<(), axum::Error> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => send_room_error(sender, error).await,
    }
}
