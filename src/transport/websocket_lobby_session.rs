//! WebSocket session loop for one connected lobby client.
//!
//! The lobby socket owns roster presence, chat, and proposed-game updates. It
//! does not relay gameplay input or snapshot bytes.

use crate::http::AppServices;
use crate::lobbies::{JoinLobbyParams, LobbyEvent, MAX_LOBBY_PLAYERS};
use crate::protocol::{LobbyClientMessage, LobbyServerMessage};
use crate::rooms::{ConnectionId, InviteCode, PlayerIndex};
use crate::transport::WebSocketLobbyJoinRequest;
use crate::transport::websocket_lobby_outbound::{
    LobbySocketSender, send_lobby_error, send_lobby_server_message, send_lobby_static_error,
    send_lobby_upgrade_error,
};
use crate::transport::websocket_lobby_rom_relay_handler::handle_lobby_rom_relay_request;
use crate::transport::websocket_peer_close::{peer_close_detail, peer_error_detail};
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use futures_util::stream::SplitStream;

/// Handles one upgraded lobby WebSocket until the client disconnects.
pub async fn handle_websocket_lobby_session(
    socket: WebSocket,
    services: AppServices,
    request: WebSocketLobbyJoinRequest,
) {
    let connection_id = ConnectionId::new();
    let mut events = match services
        .lobbies
        .subscribe_lobby(request.invite_code.clone())
        .await
    {
        Ok(events) => events,
        Err(error) => {
            send_lobby_upgrade_error(socket, error).await;
            return;
        }
    };
    let reconnect = match (
        request.reconnect_player_index,
        request.reconnect_lobby_epoch,
        request.resume_token.clone(),
    ) {
        (Some(player_index), Some(lobby_epoch), Some(resume_token)) => {
            Some((player_index, lobby_epoch, resume_token))
        }
        _ => None,
    };
    let join = if let Some((player_index, lobby_epoch, resume_token)) = reconnect {
        services
            .lobbies
            .reconnect_lobby_player(
                request.invite_code.clone(),
                request.license.clone(),
                JoinLobbyParams {
                    display_name: request.display_name.clone(),
                    capabilities: request.capabilities.clone(),
                },
                player_index,
                lobby_epoch,
                resume_token,
                connection_id,
            )
            .await
    } else {
        services
            .lobbies
            .connect_lobby(
                request.invite_code.clone(),
                request.license.clone(),
                JoinLobbyParams {
                    display_name: request.display_name.clone(),
                    capabilities: request.capabilities.clone(),
                },
                connection_id,
            )
            .await
    };
    let join = match join {
        Ok(join) => join,
        Err(error) => {
            send_lobby_upgrade_error(socket, error).await;
            return;
        }
    };
    let (mut sender, mut receiver) = socket.split();

    if send_lobby_server_message(
        &mut sender,
        &LobbyServerMessage::LobbyJoined {
            event_seq: join.lobby.event_seq,
            lobby_epoch: join.lobby.lobby_epoch,
            your_player_index: join.player_index.zero_based(),
            resume_token: join.resume_token,
            voice: join.voice,
            lobby: join.lobby,
        },
    )
    .await
    .is_err()
    {
        let _ = services
            .lobbies
            .disconnect_lobby(request.invite_code, connection_id)
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
        .lobbies
        .disconnect_lobby(request.invite_code, connection_id)
        .await;
}

async fn session_loop(
    sender: &mut LobbySocketSender,
    receiver: &mut SplitStream<WebSocket>,
    events: &mut crate::lobbies::LobbyEventReceiver,
    services: &AppServices,
    invite_code: &InviteCode,
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
                if !handle_lobby_event(sender, connection_id, event).await {
                    break;
                }
            }
        }
    }
}

async fn handle_incoming(
    sender: &mut LobbySocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    incoming: Option<Result<Message, axum::Error>>,
) -> bool {
    match incoming {
        Some(Ok(Message::Text(payload))) => {
            handle_lobby_text(
                sender,
                services,
                invite_code,
                connection_id,
                payload.as_str(),
            )
            .await
        }
        Some(Ok(Message::Close(frame))) => {
            let _detail = peer_close_detail(frame);
            false
        }
        None => false,
        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => true,
        Some(Ok(Message::Binary(_))) => send_lobby_static_error(
            sender,
            "unsupportedMessage",
            "Binary messages are not supported.",
        )
        .await
        .is_ok(),
        Some(Err(error)) => {
            let _detail = peer_error_detail(&error);
            false
        }
    }
}

async fn handle_lobby_text(
    sender: &mut LobbySocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    payload: &str,
) -> bool {
    if payload.len() > crate::limits::MAX_WEBSOCKET_MESSAGE_BYTES {
        return send_lobby_static_error(sender, "messageTooLarge", "Message is too large.")
            .await
            .is_ok();
    }

    match serde_json::from_str::<LobbyClientMessage>(payload) {
        Ok(message) => handle_lobby_message(sender, services, invite_code, connection_id, message)
            .await
            .is_ok(),
        Err(_) => send_lobby_static_error(sender, "invalidMessage", "Message JSON is invalid.")
            .await
            .is_ok(),
    }
}

async fn handle_lobby_message(
    sender: &mut LobbySocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    message: LobbyClientMessage,
) -> Result<(), axum::Error> {
    match message {
        LobbyClientMessage::Ping => {
            send_lobby_server_message(sender, &LobbyServerMessage::Pong).await
        }
        LobbyClientMessage::SelectGame { lobby_epoch, game } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .select_lobby_game(invite_code.clone(), connection_id, game)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::SetGameReadiness {
            lobby_epoch,
            proposal_id,
            status,
            detail,
        } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .set_lobby_game_readiness(
                        invite_code.clone(),
                        connection_id,
                        proposal_id,
                        status,
                        detail,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::LaunchGame {
            lobby_epoch,
            proposal_id,
        } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .request_lobby_game_launch(invite_code.clone(), connection_id, proposal_id)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::RequestRomTransfer {
            lobby_epoch,
            proposal_id,
            receiver_player_index,
        } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            let Some(receiver) = PlayerIndex::new(receiver_player_index, MAX_LOBBY_PLAYERS) else {
                return send_lobby_static_error(
                    sender,
                    "invalidLobbyPlayerIndex",
                    "Lobby player slot is invalid.",
                )
                .await;
            };

            apply_lobby_result(
                sender,
                handle_lobby_rom_relay_request(
                    services,
                    invite_code,
                    connection_id,
                    proposal_id,
                    receiver,
                )
                .await,
            )
            .await
        }
        LobbyClientMessage::PublishGameRoom {
            lobby_epoch,
            proposal_id,
            room_invite_code,
        } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            let room_invite = match InviteCode::parse(&room_invite_code) {
                Ok(room_invite) => room_invite,
                Err(_) => {
                    return send_lobby_static_error(
                        sender,
                        "invalidRoomInviteCode",
                        "Gameplay invite code is invalid.",
                    )
                    .await;
                }
            };

            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .publish_lobby_game_room(
                        invite_code.clone(),
                        connection_id,
                        proposal_id,
                        room_invite,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::ReturnToLobby {
            lobby_epoch,
            proposal_id,
        } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .return_lobby_from_game(invite_code.clone(), connection_id, proposal_id)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::Chat { lobby_epoch, body } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .send_lobby_chat(invite_code.clone(), connection_id, body)
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::RefreshVoiceToken { lobby_epoch } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            match services
                .lobbies
                .refresh_lobby_voice_token(invite_code.clone(), connection_id)
                .await
            {
                Ok(refresh) => {
                    send_lobby_server_message(
                        sender,
                        &LobbyServerMessage::VoiceTokenRefreshed {
                            event_seq: refresh.event_seq,
                            lobby_epoch: refresh.lobby_epoch,
                            voice: refresh.voice,
                        },
                    )
                    .await
                }
                Err(error) => send_lobby_error(sender, error).await,
            }
        }
        LobbyClientMessage::Leave {
            lobby_epoch,
            reason,
        } => {
            let _reason = reason;
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .leave_lobby(invite_code.clone(), connection_id)
                    .await
                    .map(|_| ()),
            )
            .await
        }
    }
}

async fn validate_lobby_epoch(
    services: &AppServices,
    invite_code: &InviteCode,
    lobby_epoch: u64,
) -> Result<(), crate::lobbies::LobbyError> {
    let lobby = services.lobbies.lobby_view(invite_code.clone()).await?;

    if lobby.lobby_epoch != lobby_epoch {
        return Err(crate::lobbies::LobbyError::StaleLobbyEpoch);
    }

    Ok(())
}

async fn apply_lobby_result(
    sender: &mut LobbySocketSender,
    result: Result<(), crate::lobbies::LobbyError>,
) -> Result<(), axum::Error> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => send_lobby_error(sender, error).await,
    }
}

async fn handle_lobby_event(
    sender: &mut LobbySocketSender,
    connection_id: ConnectionId,
    event: Result<LobbyEvent, tokio::sync::broadcast::error::RecvError>,
) -> bool {
    let message = match event {
        Ok(LobbyEvent::LobbyStateChanged(lobby)) => LobbyServerMessage::LobbyStateChanged {
            event_seq: lobby.event_seq,
            lobby_epoch: lobby.lobby_epoch,
            lobby,
        },
        Ok(LobbyEvent::ChatMessage(message)) => LobbyServerMessage::ChatMessage { message },
        Ok(LobbyEvent::RomTransferUploadGranted {
            source,
            lobby_epoch,
            grant,
        }) => {
            if source != connection_id {
                return true;
            }
            LobbyServerMessage::RomTransferUploadGranted { lobby_epoch, grant }
        }
        Ok(LobbyEvent::RomTransferDownloadReady {
            receiver,
            lobby_epoch,
            grant,
        }) => {
            if receiver != connection_id {
                return true;
            }
            LobbyServerMessage::RomTransferDownloadReady { lobby_epoch, grant }
        }
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
            return send_lobby_static_error(sender, "eventLagged", "Lobby event stream lagged.")
                .await
                .is_ok();
        }
        Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
    };

    send_lobby_server_message(sender, &message).await.is_ok()
}
