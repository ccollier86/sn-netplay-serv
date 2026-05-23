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
use crate::transport::websocket_peer_close::{peer_close_detail, peer_error_detail};
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
    let reconnect = match (
        request.reconnect_player_index,
        request.reconnect_room_epoch,
        request.resume_token.clone(),
    ) {
        (Some(player_index), Some(room_epoch), Some(resume_token)) => {
            Some((player_index, room_epoch, resume_token))
        }
        _ => None,
    };
    let join = if let Some((player_index, room_epoch, resume_token)) = reconnect.clone() {
        services
            .rooms
            .reconnect_player(
                request.invite_code.clone(),
                player_index,
                room_epoch,
                resume_token,
                connection_id,
            )
            .await
    } else {
        match request.role {
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
    if reconnect.is_some() {
        services.metrics.record_player_reconnected();
    }
    let (mut sender, mut receiver) = socket.split();

    if send_server_message(
        &mut sender,
        &ServerMessage::RoomJoined {
            event_seq: join.room.event_seq,
            room_epoch: join.room.room_epoch,
            session_epoch: join.room.session_epoch,
            your_player_index: join.player_index.zero_based(),
            input_socket_token: join.input_socket_token,
            resume_token: join.resume_token,
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
                if !handle_room_event(sender, services, connection_id, event).await {
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
        Some(Ok(Message::Close(frame))) => {
            record_transport_close(
                services,
                invite_code,
                connection_id,
                peer_close_detail(frame),
            )
            .await;
            false
        }
        None => {
            record_transport_close(
                services,
                invite_code,
                connection_id,
                "peer stream ended without close frame".to_string(),
            )
            .await;
            false
        }
        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => true,
        Some(Ok(Message::Binary(_))) => send_static_error(
            sender,
            "unsupportedMessage",
            "Binary messages are not supported.",
        )
        .await
        .is_ok(),
        Some(Err(error)) => {
            record_transport_close(
                services,
                invite_code,
                connection_id,
                peer_error_detail(&error),
            )
            .await;
            false
        }
    }
}

async fn record_transport_close(
    services: &AppServices,
    invite_code: &crate::rooms::InviteCode,
    connection_id: ConnectionId,
    reason: String,
) {
    let _ = services
        .rooms
        .record_transport_close(invite_code.clone(), connection_id, "control", reason)
        .await;
}

async fn handle_room_event(
    sender: &mut SocketSender,
    services: &AppServices,
    connection_id: ConnectionId,
    event: Result<RoomEvent, tokio::sync::broadcast::error::RecvError>,
) -> bool {
    let message = match event {
        Ok(RoomEvent::RoomStateChanged(room)) => room_state_message(room),
        Ok(RoomEvent::SessionStarted { start_frame, room }) => {
            services.metrics.record_session_started();
            ServerMessage::StartSession {
                event_seq: room.event_seq,
                room_epoch: room.room_epoch,
                session_epoch: room.session_epoch,
                start_frame,
                room,
            }
        }
        Ok(RoomEvent::SessionPauseScheduled { pause, room }) => {
            ServerMessage::SessionPauseScheduled {
                event_seq: room.event_seq,
                room_epoch: room.room_epoch,
                session_epoch: room.session_epoch,
                pause,
                room,
            }
        }
        Ok(RoomEvent::SessionPauseUpdated { pause, room }) => ServerMessage::SessionPauseUpdated {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            pause,
            room,
        },
        Ok(RoomEvent::SessionResumeScheduled {
            sequence,
            resume_at_frame,
            room,
        }) => ServerMessage::SessionResumeScheduled {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            sequence,
            resume_at_frame,
            room,
        },
        Ok(RoomEvent::PlayerExited {
            player_index,
            reason,
            room,
        }) => ServerMessage::PlayerExited {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            player_index,
            reason,
            room,
        },
        Ok(RoomEvent::StateHashMismatch { mismatch, room }) => ServerMessage::StateHashMismatch {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            mismatch,
            room,
        },
        Ok(RoomEvent::InputDelayChanged { change, room }) => ServerMessage::InputDelayChanged {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            change,
            room,
        },
        Ok(RoomEvent::InputFrame { .. }) => {
            return true;
        }
        Ok(RoomEvent::LinkCablePacket { source, packet }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::LinkCablePacket { packet }
        }
        Ok(RoomEvent::SnapshotChunk {
            source,
            room_epoch,
            session_epoch,
            chunk,
        }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::SnapshotChunk {
                room_epoch,
                session_epoch,
                chunk,
            }
        }
        Ok(RoomEvent::SnapshotComplete {
            source,
            room_epoch,
            session_epoch,
            manifest,
        }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::SnapshotComplete {
                room_epoch,
                session_epoch,
                manifest,
            }
        }
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => ServerMessage::Error {
            code: "roomEventLagged".to_string(),
            message: "Room updates were missed; refresh room state.".to_string(),
        },
        Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
    };

    send_server_message(sender, &message).await.is_ok()
}

fn room_state_message(room: crate::rooms::RoomView) -> ServerMessage {
    match room.status {
        crate::rooms::RoomStatus::CheckingCompatibility => ServerMessage::CompatibilityRequested {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            room,
        },
        crate::rooms::RoomStatus::Recovering => ServerMessage::RecoveryStarted {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            room,
        },
        _ => ServerMessage::RoomStateChanged {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            room,
        },
    }
}
