//! WebSocket session loop for one connected Desktop client.
//!
//! The session loop attaches the socket to a room slot, sends room updates, and
//! relays room events, and delegates incoming gameplay messages to a separate
//! handler.

use crate::http::AppServices;
use crate::protocol::ServerMessage;
use crate::rooms::{ConnectionId, RoomEvent};
use crate::transport::websocket_message_handler::handle_client_text;
use crate::transport::websocket_outbound::{
    SocketSender, send_server_message, send_static_error, send_upgrade_error,
};
use crate::transport::{WebSocketJoinRequest, WebSocketJoinRole};
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use futures_util::stream::SplitStream;

/// Handles one upgraded WebSocket until the client disconnects.
pub async fn handle_websocket_session(
    socket: WebSocket,
    services: AppServices,
    request: WebSocketJoinRequest,
) {
    let connection_id = ConnectionId::new();
    let mut events = match services.rooms.subscribe(request.invite_code.clone()).await {
        Ok(events) => events,
        Err(error) => {
            send_upgrade_error(socket, error).await;
            return;
        }
    };
    let join = match request.role {
        WebSocketJoinRole::Host => {
            services
                .rooms
                .connect_host(request.invite_code.clone(), request.license, connection_id)
                .await
        }
        WebSocketJoinRole::Guest => {
            services
                .rooms
                .connect_guest(request.invite_code.clone(), request.license, connection_id)
                .await
        }
    };
    let join = match join {
        Ok(join) => join,
        Err(error) => {
            send_upgrade_error(socket, error).await;
            return;
        }
    };
    services.metrics.record_websocket_joined();
    let (mut sender, mut receiver) = socket.split();

    if send_server_message(
        &mut sender,
        &ServerMessage::RoomJoined {
            your_player_index: join.player_index.zero_based(),
            room: join.room,
        },
    )
    .await
    .is_err()
    {
        let _ = services
            .rooms
            .disconnect(request.invite_code, connection_id)
            .await;
        return;
    }

    session_loop(
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
        .disconnect(request.invite_code, connection_id)
        .await;
}

async fn session_loop(
    sender: &mut SocketSender,
    receiver: &mut SplitStream<WebSocket>,
    events: &mut crate::rooms::RoomEventReceiver,
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
        Some(Ok(Message::Text(payload))) => {
            handle_client_text(
                sender,
                services,
                invite_code,
                connection_id,
                payload.as_str(),
            )
            .await
        }
        Some(Ok(Message::Close(_))) | None => false,
        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => true,
        Some(Ok(Message::Binary(_))) => send_static_error(
            sender,
            "unsupportedMessage",
            "Binary messages are not supported.",
        )
        .await
        .is_ok(),
        Some(Err(_)) => false,
    }
}

async fn handle_room_event(
    sender: &mut SocketSender,
    connection_id: ConnectionId,
    event: Result<RoomEvent, tokio::sync::broadcast::error::RecvError>,
) -> bool {
    let message = match event {
        Ok(RoomEvent::RoomStateChanged(room)) => ServerMessage::RoomStateChanged { room },
        Ok(RoomEvent::SessionStarted { start_frame, room }) => {
            ServerMessage::StartSession { start_frame, room }
        }
        Ok(RoomEvent::InputFrame { input, .. }) => ServerMessage::InputFrame { input },
        Ok(RoomEvent::SnapshotChunk { source, chunk }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::SnapshotChunk { chunk }
        }
        Ok(RoomEvent::SnapshotComplete { source, manifest }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::SnapshotComplete { manifest }
        }
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => ServerMessage::Error {
            code: "roomEventLagged".to_string(),
            message: "Room updates were missed; refresh room state.".to_string(),
        },
        Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
    };

    send_server_message(sender, &message).await.is_ok()
}
