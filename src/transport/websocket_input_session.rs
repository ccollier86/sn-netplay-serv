//! Binary input WebSocket session loop.
//!
//! The control socket owns JSON room lifecycle traffic. This input socket accepts
//! only binary input batches and relays only binary input batches to peers.

use crate::http::AppServices;
use crate::protocol::{decode_input_frame_batch, encode_input_frame_batch};
use crate::rooms::{ConnectionId, RoomInputEvent};
use crate::transport::WebSocketInputJoinRequest;
use crate::transport::websocket_outbound::{
    SocketSender, send_binary_message, send_static_error, send_upgrade_error,
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use futures_util::stream::SplitStream;

/// Handles one upgraded binary input WebSocket until disconnect.
pub async fn handle_websocket_input_session(
    socket: WebSocket,
    services: AppServices,
    request: WebSocketInputJoinRequest,
) {
    let connection_id = ConnectionId::new();
    let mut events = match services
        .rooms
        .subscribe_input(request.invite_code.clone())
        .await
    {
        Ok(events) => events,
        Err(error) => {
            send_upgrade_error(socket, error).await;
            return;
        }
    };
    let room = match services
        .rooms
        .connect_input_socket(
            request.invite_code.clone(),
            request.player_index,
            request.room_epoch,
            request.session_epoch,
            request.input_socket_token,
            connection_id,
        )
        .await
    {
        Ok(room) => room,
        Err(error) => {
            send_upgrade_error(socket, error).await;
            return;
        }
    };
    let (mut sender, mut receiver) = socket.split();

    let _ = room;
    input_session_loop(
        &mut sender,
        &mut receiver,
        &mut events,
        &services,
        &request.invite_code,
        connection_id,
    )
    .await;

    let _ = services
        .rooms
        .disconnect_input_socket(request.invite_code, connection_id)
        .await;
}

async fn input_session_loop(
    sender: &mut SocketSender,
    receiver: &mut SplitStream<WebSocket>,
    events: &mut crate::rooms::RoomInputEventReceiver,
    services: &AppServices,
    invite_code: &crate::rooms::InviteCode,
    connection_id: ConnectionId,
) {
    loop {
        tokio::select! {
            incoming = receiver.next() => {
                if !handle_incoming(sender, services, invite_code, connection_id, incoming).await {
                    break;
                }
            }
            event = events.recv() => {
                if !handle_room_event(sender, connection_id, event).await {
                    break;
                }
            }
        }
    }
}

async fn handle_incoming(
    sender: &mut SocketSender,
    services: &AppServices,
    invite_code: &crate::rooms::InviteCode,
    connection_id: ConnectionId,
    incoming: Option<Result<Message, axum::Error>>,
) -> bool {
    match incoming {
        Some(Ok(Message::Binary(payload))) => {
            let batch = match decode_input_frame_batch(&payload) {
                Ok(batch) => batch,
                Err(_) => {
                    services.metrics.record_protocol_error();
                    return send_static_error(
                        sender,
                        "invalidInputBatch",
                        "Input batch is invalid.",
                    )
                    .await
                    .is_ok();
                }
            };
            services
                .rooms
                .relay_input_frame_batch(invite_code.clone(), connection_id, batch)
                .await
                .map(|_| true)
                .unwrap_or_else(|_| false)
        }
        Some(Ok(Message::Close(_))) | None => false,
        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => true,
        Some(Ok(Message::Text(_))) => send_static_error(
            sender,
            "unsupportedMessage",
            "Input socket only accepts binary input batches.",
        )
        .await
        .is_ok(),
        Some(Err(_)) => false,
    }
}

async fn handle_room_event(
    sender: &mut SocketSender,
    connection_id: ConnectionId,
    event: Result<RoomInputEvent, tokio::sync::broadcast::error::RecvError>,
) -> bool {
    match event {
        Ok(RoomInputEvent::InputFrameBatch { source, batch }) => {
            if source == connection_id {
                return true;
            }

            let payload = match encode_input_frame_batch(&batch) {
                Ok(payload) => payload,
                Err(_) => return false,
            };
            send_binary_message(sender, payload).await.is_ok()
        }
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => false,
        Err(tokio::sync::broadcast::error::RecvError::Closed) => false,
    }
}
