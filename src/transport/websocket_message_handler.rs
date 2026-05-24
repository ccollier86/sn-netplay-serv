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
use crate::transport::websocket_voice_handler::handle_refresh_voice_token;

/// Parses and applies one client text message.
pub async fn handle_client_text(
    sender: &mut SocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    payload: &str,
) -> bool {
    if payload.len() > MAX_WEBSOCKET_MESSAGE_BYTES {
        services.metrics.record_protocol_error();
        return send_static_error(sender, "messageTooLarge", "Message is too large.")
            .await
            .is_ok();
    }

    match serde_json::from_str::<ClientMessage>(payload) {
        Ok(message) => handle_client_message(sender, services, invite_code, connection_id, message)
            .await
            .is_ok(),
        Err(_) => {
            services.metrics.record_protocol_error();
            send_static_error(sender, "invalidMessage", "Message JSON is invalid.")
                .await
                .is_ok()
        }
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
        ClientMessage::SetCompatibilityFingerprint {
            room_epoch,
            session_epoch,
            fingerprint,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
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
        ClientMessage::SetLinkCableCompatibility {
            room_epoch,
            session_epoch,
            compatibility,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
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
        ClientMessage::Ready {
            room_epoch,
            session_epoch,
            network,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .mark_ready(invite_code.clone(), connection_id, network)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        ClientMessage::SnapshotChunk {
            room_epoch,
            session_epoch,
            chunk,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .relay_snapshot_chunk(invite_code.clone(), connection_id, chunk)
                    .await,
            )
            .await
        }
        ClientMessage::SnapshotComplete {
            room_epoch,
            session_epoch,
            manifest,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .relay_snapshot_complete(invite_code.clone(), connection_id, manifest)
                    .await,
            )
            .await
        }
        ClientMessage::InputFrame {
            room_epoch,
            session_epoch,
            input,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .relay_input_frame(invite_code.clone(), connection_id, input)
                    .await,
            )
            .await
        }
        ClientMessage::LinkCablePacket {
            room_epoch,
            session_epoch,
            packet,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .relay_link_cable_packet(invite_code.clone(), connection_id, packet)
                    .await,
            )
            .await
        }
        ClientMessage::Heartbeat {
            room_epoch,
            session_epoch,
            latest_event_seq,
            local_frame,
            runtime_state,
            network,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            let room = match services
                .rooms
                .record_heartbeat(
                    invite_code.clone(),
                    connection_id,
                    latest_event_seq,
                    local_frame,
                    runtime_state,
                    network,
                )
                .await
            {
                Ok(room) => {
                    services.metrics.record_heartbeat();
                    room
                }
                Err(error) => {
                    return apply_room_result(sender, Err(error)).await;
                }
            };
            send_server_message(
                sender,
                &ServerMessage::HeartbeatAck {
                    event_seq: room.event_seq,
                    room_epoch: room.room_epoch,
                    session_epoch: room.session_epoch,
                },
            )
            .await
        }
        ClientMessage::RequestSessionPause {
            room_epoch,
            session_epoch,
            request_id,
            reason,
            local_frame,
        } => {
            services.metrics.record_pause_requested();
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .request_session_pause(
                        invite_code.clone(),
                        connection_id,
                        request_id,
                        reason,
                        local_frame,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        ClientMessage::SessionPauseReached {
            room_epoch,
            session_epoch,
            sequence,
            paused_at_frame,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .mark_session_pause_reached(
                        invite_code.clone(),
                        connection_id,
                        sequence,
                        paused_at_frame,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        ClientMessage::RequestSessionResume {
            room_epoch,
            session_epoch,
            request_id,
            reason,
            sequence,
        } => {
            services.metrics.record_resume_requested();
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .request_session_resume(
                        invite_code.clone(),
                        connection_id,
                        request_id,
                        reason,
                        sequence,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        ClientMessage::PlayerExited {
            room_epoch,
            session_epoch,
            reason,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .player_exited(invite_code.clone(), connection_id, reason)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        ClientMessage::RefreshVoiceToken {
            room_epoch,
            session_epoch,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            handle_refresh_voice_token(sender, services, invite_code, connection_id).await
        }
        ClientMessage::StateHash {
            room_epoch,
            session_epoch,
            report,
        } => {
            apply_room_result(
                sender,
                validate_epochs(services, invite_code, room_epoch, session_epoch).await,
            )
            .await?;
            apply_room_result(
                sender,
                services
                    .rooms
                    .record_state_hash(invite_code.clone(), connection_id, report)
                    .await,
            )
            .await
        }
    }
}

async fn validate_epochs(
    services: &AppServices,
    invite_code: &InviteCode,
    room_epoch: u64,
    session_epoch: u64,
) -> Result<(), RoomError> {
    let room = services.rooms.room_view(invite_code.clone()).await?;

    if room.room_epoch != room_epoch {
        return Err(RoomError::StaleRoomEpoch);
    }

    if room.session_epoch != session_epoch {
        return Err(RoomError::StaleSessionEpoch);
    }

    Ok(())
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
