//! WebSocket session loop for one connected Desktop client.
//!
//! The session loop attaches the socket to a room slot, sends room updates, and
//! relays room events, and delegates incoming gameplay messages to a separate
//! handler.

use crate::http::AppServices;
use crate::protocol::{
    LINK_CABLE_CONTRACT_VERSION, LinkCableAbortReason, LinkCableDataPlaneGrant,
    LinkCableGrantFailureReason, LinkCableGrantStatus, MAX_LINK_CABLE_WIRE_BYTES, ServerMessage,
};
use crate::rooms::{
    ClientTransportCapabilities, ConnectionId, LinkCableDataPlaneError, LinkCableDataPlaneEvent,
    LinkCableDataPlaneReceiver, LinkCableDataPlaneSnapshot, LinkCableDataPlaneStatus, RoomEvent,
};
use crate::transport::websocket_message_handler::handle_client_text;
use crate::transport::websocket_outbound::{
    SocketSender, send_server_message, send_static_error, send_upgrade_error,
};
use crate::transport::websocket_peer_close::{peer_close_detail, peer_error_detail};
use crate::transport::{WebSocketJoinRequest, WebSocketJoinRole, WebSocketRoomJoinIntent};
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use futures_util::stream::SplitStream;
use std::future::pending;

/// Handles one upgraded WebSocket until the client disconnects.
pub async fn handle_websocket_session(
    socket: WebSocket,
    services: AppServices,
    request: WebSocketJoinRequest,
) {
    let invite_code = request.invite_code;
    let connection_id = ConnectionId::new();
    let capabilities = ClientTransportCapabilities {
        supports_state_file_relay: request.supports_state_file_relay,
        supports_rom_file_relay: request.supports_rom_file_relay,
        supports_scheduled_start: request.supports_scheduled_start,
        supports_clock_sync: request.supports_clock_sync,
        supports_fast_input_relay: request.supports_fast_input_relay,
    };
    let mut events = match services.rooms.subscribe(invite_code.clone()).await {
        Ok(events) => events,
        Err(error) => {
            send_upgrade_error(socket, error).await;
            return;
        }
    };
    let (join, reconnecting, runner_handoff) = match request.intent {
        WebSocketRoomJoinIntent::Resume {
            player_index,
            room_epoch,
            resume_token,
        } => (
            services
                .rooms
                .reconnect_player(
                    invite_code.clone(),
                    player_index,
                    room_epoch,
                    resume_token,
                    connection_id,
                    capabilities,
                )
                .await,
            true,
            false,
        ),
        WebSocketRoomJoinIntent::Initial {
            role,
            license,
            runner_handoff,
        } => {
            let join = match role {
                WebSocketJoinRole::Host => {
                    services
                        .rooms
                        .connect_host(invite_code.clone(), license, connection_id, capabilities)
                        .await
                }
                WebSocketJoinRole::Guest => {
                    services
                        .rooms
                        .connect_guest(invite_code.clone(), license, connection_id, capabilities)
                        .await
                }
            };
            (join, false, runner_handoff)
        }
    };
    let join = match join {
        Ok(join) => join,
        Err(error) => {
            send_upgrade_error(socket, error).await;
            return;
        }
    };

    let link_attachment = match services
        .rooms
        .claim_link_cable_data_plane(invite_code.clone(), connection_id)
        .await
    {
        Ok(attachment) => attachment,
        Err(error) => {
            let _ = services
                .rooms
                .disconnect(invite_code.clone(), connection_id)
                .await;
            send_upgrade_error(socket, error).await;
            return;
        }
    };
    let initial_link_grant = link_attachment
        .as_ref()
        .map(|attachment| link_data_plane_grant(attachment.snapshot));

    services.metrics.record_websocket_joined();
    if reconnecting {
        services.metrics.record_player_reconnected();
    }

    if runner_handoff
        && let Err(error) = services
            .rooms
            .arm_runner_handoff(invite_code.clone(), connection_id)
            .await
    {
        let _ = services
            .rooms
            .disconnect(invite_code.clone(), connection_id)
            .await;
        send_upgrade_error(socket, error).await;
        return;
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
            voice: join.voice,
            link_cable_grant: initial_link_grant,
            room: join.room,
        },
    )
    .await
    .is_err()
    {
        if runner_handoff {
            let _ = services
                .rooms
                .cancel_runner_handoff(invite_code.clone(), connection_id)
                .await;
        }
        let _ = services.rooms.disconnect(invite_code, connection_id).await;
        return;
    }

    let mut link_receiver = link_attachment.map(|attachment| attachment.receiver);
    session_loop(
        &mut sender,
        &mut receiver,
        &mut events,
        &mut link_receiver,
        &services,
        &invite_code,
        connection_id,
    )
    .await;

    let _ = services.rooms.disconnect(invite_code, connection_id).await;
}

async fn session_loop(
    sender: &mut SocketSender,
    receiver: &mut SplitStream<WebSocket>,
    events: &mut crate::rooms::RoomEventReceiver,
    link_receiver: &mut Option<LinkCableDataPlaneReceiver>,
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
            link_event = next_link_data_plane_event(link_receiver) => {
                if !handle_link_data_plane_event(sender, link_receiver, link_event).await {
                    break;
                }
            }
        }
    }
}

async fn next_link_data_plane_event(
    receiver: &mut Option<LinkCableDataPlaneReceiver>,
) -> Result<LinkCableDataPlaneEvent, LinkCableDataPlaneError> {
    match receiver {
        Some(receiver) => receiver.recv().await,
        None => pending().await,
    }
}

async fn handle_link_data_plane_event(
    sender: &mut SocketSender,
    receiver: &mut Option<LinkCableDataPlaneReceiver>,
    event: Result<LinkCableDataPlaneEvent, LinkCableDataPlaneError>,
) -> bool {
    let confirms_packet_delivery = matches!(&event, Ok(LinkCableDataPlaneEvent::Packet(_)));
    let message = match event {
        Ok(LinkCableDataPlaneEvent::Packet(packet)) => ServerMessage::LinkCablePacket { packet },
        Ok(LinkCableDataPlaneEvent::Lifecycle(snapshot)) => ServerMessage::LinkCableGrantUpdated {
            grant: link_data_plane_grant(snapshot),
        },
        Err(LinkCableDataPlaneError::Closed) => {
            *receiver = None;
            return true;
        }
        Err(_) => {
            *receiver = None;
            return send_static_error(
                sender,
                "linkCableRouteClosed",
                "The private link-cable route closed.",
            )
            .await
            .is_ok();
        }
    };

    if send_server_message(sender, &message).await.is_err() {
        return false;
    }
    if confirms_packet_delivery
        && receiver
            .as_mut()
            .is_some_and(|receiver| receiver.confirm_packet_delivery().is_err())
    {
        return false;
    }

    true
}

fn link_data_plane_grant(snapshot: LinkCableDataPlaneSnapshot) -> LinkCableDataPlaneGrant {
    let (status, failure_reason) = match snapshot.status {
        LinkCableDataPlaneStatus::Waiting => (LinkCableGrantStatus::WaitingForPeer, None),
        LinkCableDataPlaneStatus::Active => (LinkCableGrantStatus::Ready, None),
        LinkCableDataPlaneStatus::Aborted => (
            LinkCableGrantStatus::Aborted,
            Some(match snapshot.abort_reason {
                Some(LinkCableAbortReason::QueueOverflow) => {
                    LinkCableGrantFailureReason::QueueOverflow
                }
                Some(LinkCableAbortReason::PeerDisconnected) => {
                    LinkCableGrantFailureReason::PeerDisconnected
                }
                Some(LinkCableAbortReason::ProtocolViolation | LinkCableAbortReason::Timeout) => {
                    LinkCableGrantFailureReason::ProtocolViolation
                }
                Some(LinkCableAbortReason::CoreClosed) | None => {
                    LinkCableGrantFailureReason::ProviderReset
                }
            }),
        ),
        LinkCableDataPlaneStatus::Closed => (
            LinkCableGrantStatus::Closed,
            Some(LinkCableGrantFailureReason::RouteClosed),
        ),
    };

    LinkCableDataPlaneGrant {
        contract_version: LINK_CABLE_CONTRACT_VERSION,
        room_scope: snapshot.room_scope.to_string(),
        room_epoch: snapshot.room_epoch,
        session_epoch: snapshot.session_epoch,
        cable_epoch: snapshot.cable_epoch,
        local_slot: snapshot.local_slot.zero_based(),
        link_protocol: snapshot.protocol.wire_value().to_string(),
        maximum_event_bytes: u16::try_from(MAX_LINK_CABLE_WIRE_BYTES)
            .expect("SBLK maximum frame size fits in u16"),
        queue_capacity: u16::try_from(snapshot.queue_capacity)
            .expect("configured link queue capacity fits in u16"),
        status,
        failure_reason,
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
        Ok(RoomEvent::SessionStarted {
            start_frame,
            scheduled_start,
            room,
        }) => {
            services.metrics.record_session_started();
            ServerMessage::StartSession {
                event_seq: room.event_seq,
                room_epoch: room.room_epoch,
                session_epoch: room.session_epoch,
                start_frame,
                scheduled_start,
                room,
            }
        }
        Ok(RoomEvent::ClockSyncSampleRequested {
            request,
            room_epoch,
            session_epoch,
        }) => ServerMessage::ClockSyncSampleRequested {
            room_epoch,
            session_epoch,
            request,
        },
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
            scheduled_start,
            room,
        }) => ServerMessage::SessionResumeScheduled {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            sequence,
            resume_at_frame,
            scheduled_start,
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
        Ok(RoomEvent::StateRecoveryPrepare { recovery, room }) => {
            ServerMessage::StateRecoveryPrepare {
                event_seq: room.event_seq,
                room_epoch: room.room_epoch,
                session_epoch: room.session_epoch,
                recovery,
                room,
            }
        }
        Ok(RoomEvent::StateRecoveryCommitted { recovery, room }) => {
            ServerMessage::StateRecoveryCommitted {
                event_seq: room.event_seq,
                room_epoch: room.room_epoch,
                session_epoch: room.session_epoch,
                recovery,
                room,
            }
        }
        Ok(RoomEvent::StateRecoveryFailed {
            recovery,
            reason,
            room,
        }) => ServerMessage::StateRecoveryFailed {
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            recovery,
            reason,
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
        Ok(RoomEvent::SnapshotFileRelayUploadGranted {
            source,
            room_epoch,
            session_epoch,
            grant,
        }) => {
            if source != connection_id {
                return true;
            }
            ServerMessage::SnapshotFileRelayUploadGranted {
                room_epoch,
                session_epoch,
                grant,
            }
        }
        Ok(RoomEvent::SnapshotFileRelayDownloadReady {
            receiver,
            room_epoch,
            session_epoch,
            grant,
        }) => {
            if receiver != connection_id {
                return true;
            }
            ServerMessage::SnapshotFileRelayDownloadReady {
                room_epoch,
                session_epoch,
                grant,
            }
        }
        Ok(RoomEvent::RomRelayUploadGranted {
            source,
            room_epoch,
            session_epoch,
            grant,
        }) => {
            if source != connection_id {
                return true;
            }
            ServerMessage::RomRelayGrantUpload {
                room_epoch,
                session_epoch,
                grant,
            }
        }
        Ok(RoomEvent::RomRelayDownloadGranted {
            receiver,
            room_epoch,
            session_epoch,
            grant,
        }) => {
            if receiver != connection_id {
                return true;
            }
            ServerMessage::RomRelayGrantDownload {
                room_epoch,
                session_epoch,
                grant,
            }
        }
        Ok(RoomEvent::RomRelayProgress {
            source,
            room_epoch,
            session_epoch,
            progress,
        }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::RomRelayProgress {
                room_epoch,
                session_epoch,
                progress,
            }
        }
        Ok(RoomEvent::RomRelayCompleted {
            source,
            room_epoch,
            session_epoch,
            completion,
        }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::RomRelayCompleted {
                room_epoch,
                session_epoch,
                completion,
            }
        }
        Ok(RoomEvent::RomRelayFailed {
            source,
            room_epoch,
            session_epoch,
            failure,
        }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::RomRelayFailed {
                room_epoch,
                session_epoch,
                failure,
            }
        }
        Ok(RoomEvent::RomRelayCancelled {
            source,
            room_epoch,
            session_epoch,
            cancelled,
        }) => {
            if source == connection_id {
                return true;
            }
            ServerMessage::RomRelayCancelled {
                room_epoch,
                session_epoch,
                cancelled,
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
