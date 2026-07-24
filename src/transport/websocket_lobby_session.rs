//! WebSocket session loop for one connected lobby client.
//!
//! The lobby socket owns roster presence, chat, and proposed-game updates. It
//! does not relay gameplay input or snapshot bytes.

use crate::http::AppServices;
use crate::lobbies::{
    JoinLobbyParams, LOBBY_LINK_CABLE_CONTRACT_VERSION, LobbyClientCapabilities, LobbyError,
    LobbyEvent, LobbyGameLaunchStatus, LobbyLinkProtocolFamily, LobbyView,
    MAX_LINK_CABLE_LOBBY_PLAYERS, MAX_LOBBY_PLAYERS, ReconnectLobbyPlayerRequest,
};
use crate::protocol::{
    LobbyClientMessage, LobbyServerMessage, NetplaySessionDescriptor, NetplaySessionMode,
};
use crate::rooms::{ConnectionId, InviteCode, PlayerIndex, RoomStatus};
use crate::transport::WebSocketLobbyJoinRequest;
use crate::transport::websocket_lobby_outbound::{
    LobbySocketSender, send_lobby_error, send_lobby_server_message, send_lobby_static_error,
    send_lobby_upgrade_error,
};
use crate::transport::websocket_lobby_rom_relay_handler::handle_lobby_rom_relay_request;
use crate::transport::websocket_lobby_startup_state_relay_handler::handle_lobby_startup_state_relay_request;
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
            .reconnect_lobby_player(ReconnectLobbyPlayerRequest {
                invite_code: request.invite_code.clone(),
                player: request.license.clone(),
                params: JoinLobbyParams {
                    display_name: request.display_name.clone(),
                    capabilities: request.capabilities.clone(),
                },
                player_index,
                lobby_epoch,
                resume_token,
                connection_id,
            })
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
            lobby: lobby_view_for_client(join.lobby, &request.capabilities),
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
        &request.capabilities,
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
    capabilities: &LobbyClientCapabilities,
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
                if !handle_lobby_event(sender, connection_id, capabilities, event).await {
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
        LobbyClientMessage::SelectLinkCableGame {
            lobby_epoch,
            game,
            protocol_family,
            room_invite_code,
        } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            if !services.link_cable_rollout.is_enabled() {
                return send_lobby_static_error(
                    sender,
                    "linkCableUnavailable",
                    "Game Boy multiplayer is not available on this server.",
                )
                .await;
            }
            let room_invite_code = match room_invite_code.map(InviteCode::parse).transpose() {
                Ok(room_invite_code) => room_invite_code,
                Err(_) => {
                    return send_lobby_error(sender, crate::lobbies::LobbyError::InvalidPayload)
                        .await;
                }
            };
            if let Some(room_invite_code) = room_invite_code.as_ref() {
                apply_lobby_result(
                    sender,
                    validate_link_cable_room_invite(
                        services,
                        invite_code,
                        room_invite_code,
                        protocol_family,
                    )
                    .await,
                )
                .await?;
            }
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .select_lobby_link_cable_game(
                        invite_code.clone(),
                        connection_id,
                        game,
                        protocol_family,
                        room_invite_code,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::SetLinkCableLaunchState {
            lobby_epoch,
            selection_generation,
            state,
            room_invite_code,
        } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            let room_invite_code = match room_invite_code.map(InviteCode::parse).transpose() {
                Ok(room_invite_code) => room_invite_code,
                Err(_) => {
                    return send_lobby_error(sender, crate::lobbies::LobbyError::InvalidPayload)
                        .await;
                }
            };
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .set_lobby_link_cable_launch_state(
                        invite_code.clone(),
                        connection_id,
                        selection_generation,
                        state,
                        room_invite_code,
                    )
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
        LobbyClientMessage::RequestStartupStateTransfer {
            lobby_epoch,
            proposal_id,
            receiver_player_index,
            state,
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
                handle_lobby_startup_state_relay_request(
                    services,
                    invite_code,
                    connection_id,
                    proposal_id,
                    receiver,
                    state,
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
        LobbyClientMessage::GameplayStarted {
            lobby_epoch,
            proposal_id,
        } => {
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .mark_lobby_gameplay_started(
                        invite_code.clone(),
                        connection_id,
                        lobby_epoch,
                        proposal_id,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::ReturnToLobby {
            lobby_epoch,
            proposal_id,
            return_requested_by_player_index,
            reason,
        } => {
            let return_requested_by_player_index = match return_requested_by_player_index {
                Some(player_index) => match PlayerIndex::new(player_index, MAX_LOBBY_PLAYERS) {
                    Some(player_index) => Some(player_index),
                    None => {
                        return send_lobby_error(
                            sender,
                            crate::lobbies::LobbyError::InvalidPayload,
                        )
                        .await;
                    }
                },
                None => None,
            };
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .return_lobby_from_game(
                        invite_code.clone(),
                        connection_id,
                        lobby_epoch,
                        proposal_id,
                        return_requested_by_player_index,
                        reason,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::Chat {
            lobby_epoch: _,
            body,
        } => {
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
        LobbyClientMessage::RemovePlayer {
            lobby_epoch,
            player_index,
        } => {
            let Some(player_index) = PlayerIndex::new(player_index, MAX_LOBBY_PLAYERS) else {
                return send_lobby_static_error(
                    sender,
                    "invalidLobbyPlayerIndex",
                    "Lobby player slot is invalid.",
                )
                .await;
            };
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .remove_lobby_player(
                        invite_code.clone(),
                        connection_id,
                        lobby_epoch,
                        player_index,
                    )
                    .await
                    .map(|_| ()),
            )
            .await
        }
        LobbyClientMessage::ReportActivity { lobby_epoch, kind } => {
            apply_lobby_result(
                sender,
                validate_lobby_epoch(services, invite_code, lobby_epoch).await,
            )
            .await?;
            apply_lobby_result(
                sender,
                services
                    .lobbies
                    .record_lobby_activity(invite_code.clone(), connection_id, kind)
                    .await,
            )
            .await
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

async fn validate_link_cable_room_invite(
    services: &AppServices,
    lobby_invite_code: &InviteCode,
    room_invite_code: &InviteCode,
    protocol_family: LobbyLinkProtocolFamily,
) -> Result<(), LobbyError> {
    let lobby = services
        .lobbies
        .lobby_view(lobby_invite_code.clone())
        .await?;
    let host_link_capability = lobby
        .players
        .first()
        .and_then(|player| player.capabilities.link_cable.as_ref())
        .filter(|capability| {
            capability.contract_version == LOBBY_LINK_CABLE_CONTRACT_VERSION
                && capability.protocol_families.contains(&protocol_family)
        })
        .ok_or(LobbyError::LinkCableCapabilityRequired)?;
    let room = services
        .rooms
        .room_view(room_invite_code.clone())
        .await
        .map_err(|_| LobbyError::InvalidPayload)?;

    validate_link_cable_room_descriptor(
        room.status,
        room.max_players,
        &room.session,
        protocol_family,
        &host_link_capability.runtime_profile,
    )
}

fn validate_link_cable_room_descriptor(
    room_status: RoomStatus,
    room_max_players: u8,
    session: &NetplaySessionDescriptor,
    protocol_family: LobbyLinkProtocolFamily,
    expected_runtime_profile: &str,
) -> Result<(), LobbyError> {
    let (expected_system_family, expected_link_protocol) = match protocol_family {
        LobbyLinkProtocolFamily::GbSerialV1 => ("gb", "gb-serial-v1"),
        LobbyLinkProtocolFamily::GbaMultiV1 => ("gba", "gba-sio-multi-v1"),
        LobbyLinkProtocolFamily::GbaMultiV2 => ("gba", "gba-sio-multi-v2"),
    };
    let game_system_matches = match protocol_family {
        LobbyLinkProtocolFamily::GbSerialV1 => {
            matches!(session.game.system_id.as_str(), "gb" | "gbc")
        }
        LobbyLinkProtocolFamily::GbaMultiV1 => session.game.system_id == "gba",
        LobbyLinkProtocolFamily::GbaMultiV2 => session.game.system_id == "gba",
    };
    let valid = room_status == RoomStatus::WaitingForGuest
        && room_max_players == MAX_LINK_CABLE_LOBBY_PLAYERS
        && session.mode == NetplaySessionMode::LinkCable
        && session.core.core_id == "mgba"
        && game_system_matches
        && session.link.as_ref().is_some_and(|link| {
            link.max_players == MAX_LINK_CABLE_LOBBY_PLAYERS
                && link.system_family == expected_system_family
                && link.link_protocol == expected_link_protocol
                && link.runtime_profile == expected_runtime_profile
        });

    if valid {
        Ok(())
    } else {
        Err(LobbyError::InvalidPayload)
    }
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
    capabilities: &LobbyClientCapabilities,
    event: Result<LobbyEvent, tokio::sync::broadcast::error::RecvError>,
) -> bool {
    let message = match event {
        Ok(LobbyEvent::LobbyStateChanged(lobby)) => {
            let lobby = lobby_view_for_client(lobby, capabilities);
            LobbyServerMessage::LobbyStateChanged {
                event_seq: lobby.event_seq,
                lobby_epoch: lobby.lobby_epoch,
                lobby,
            }
        }
        Ok(LobbyEvent::LobbyReturned { lobby, returned }) => {
            let lobby = lobby_view_for_client(lobby, capabilities);
            if capabilities.supports_lobby_returned_event {
                LobbyServerMessage::LobbyReturned {
                    event_seq: lobby.event_seq,
                    lobby_epoch: lobby.lobby_epoch,
                    returned,
                    lobby,
                }
            } else {
                LobbyServerMessage::LobbyStateChanged {
                    event_seq: lobby.event_seq,
                    lobby_epoch: lobby.lobby_epoch,
                    lobby,
                }
            }
        }
        Ok(LobbyEvent::ChatMessage(message)) => LobbyServerMessage::ChatMessage { message },
        Ok(LobbyEvent::LobbyClosed { lobby, reason }) => {
            let lobby = lobby_view_for_client(lobby, capabilities);
            let event_seq = lobby.event_seq;
            let lobby_epoch = lobby.lobby_epoch;
            let message = LobbyServerMessage::LobbyClosed {
                event_seq,
                lobby_epoch,
                reason,
                lobby,
            };

            let _ = send_lobby_server_message(sender, &message).await;
            return false;
        }
        Ok(LobbyEvent::PlayerRemoved {
            target,
            player_index,
            reason,
            lobby,
        }) => {
            if target != connection_id {
                return true;
            }
            if capabilities.supports_lobby_player_removed_event {
                let lobby = lobby_view_for_client(lobby, capabilities);
                let message = LobbyServerMessage::PlayerRemoved {
                    event_seq: lobby.event_seq,
                    lobby_epoch: lobby.lobby_epoch,
                    player_index,
                    reason,
                    lobby,
                };
                let _ = send_lobby_server_message(sender, &message).await;
            } else {
                let _ = send_lobby_static_error(
                    sender,
                    "removedFromLobby",
                    "The host removed you from the lobby.",
                )
                .await;
            }
            return false;
        }
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
        Ok(LobbyEvent::StartupStateTransferUploadGranted {
            source,
            lobby_epoch,
            grant,
        }) => {
            if source != connection_id {
                return true;
            }
            LobbyServerMessage::StartupStateTransferUploadGranted { lobby_epoch, grant }
        }
        Ok(LobbyEvent::StartupStateTransferDownloadReady {
            receiver,
            lobby_epoch,
            grant,
        }) => {
            if receiver != connection_id {
                return true;
            }
            LobbyServerMessage::StartupStateTransferDownloadReady { lobby_epoch, grant }
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

fn lobby_view_for_client(
    mut lobby: LobbyView,
    capabilities: &LobbyClientCapabilities,
) -> LobbyView {
    if capabilities.link_cable.is_none() {
        lobby.multiplayer_extension = None;
    }
    if !capabilities.supports_lobby_gameplay_started
        && let Some(launch) = lobby.pending_launch.as_mut()
    {
        if launch.status == LobbyGameLaunchStatus::Playing {
            launch.status = LobbyGameLaunchStatus::Ready;
        }
        launch.gameplay_started_at_ms = None;
        launch.started_player_indexes.clear();
    }

    lobby
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lobbies::{
        LobbyGameLaunchView, LobbyLinkCableClientCapabilities, LobbyLinkCableLaunchState,
        LobbyLinkCablePlayerSlotView, LobbyLinkCableView, LobbyLinkProtocolFamily,
        LobbyMultiplayerExtension, LobbyMultiplayerSessionKind, LobbyServerCapabilities,
        LobbyStatus, LobbyVisibility, MAX_LOBBY_PLAYERS,
    };
    use crate::rooms::{PlayerIndex, RoomId};
    use serde_json::json;

    #[test]
    fn link_lobby_room_descriptor_requires_open_exact_mgba_provider_mapping() {
        let gba = link_session("gba", "gba", "gba-sio-multi-v2");
        validate_link_cable_room_descriptor(
            RoomStatus::WaitingForGuest,
            2,
            &gba,
            LobbyLinkProtocolFamily::GbaMultiV2,
            "mgba-link-runtime-v1",
        )
        .expect("exact GBA room accepted");

        let gbc = link_session("gbc", "gb", "gb-serial-v1");
        validate_link_cable_room_descriptor(
            RoomStatus::WaitingForGuest,
            2,
            &gbc,
            LobbyLinkProtocolFamily::GbSerialV1,
            "mgba-link-runtime-v1",
        )
        .expect("exact GB/GBC room accepted");

        assert!(
            validate_link_cable_room_descriptor(
                RoomStatus::Playing,
                2,
                &gba,
                LobbyLinkProtocolFamily::GbaMultiV2,
                "mgba-link-runtime-v1",
            )
            .is_err()
        );
        assert!(
            validate_link_cable_room_descriptor(
                RoomStatus::WaitingForGuest,
                4,
                &gba,
                LobbyLinkProtocolFamily::GbaMultiV2,
                "mgba-link-runtime-v1",
            )
            .is_err()
        );
        assert!(
            validate_link_cable_room_descriptor(
                RoomStatus::WaitingForGuest,
                2,
                &gba,
                LobbyLinkProtocolFamily::GbaMultiV2,
                "other-runtime",
            )
            .is_err()
        );
        assert!(
            validate_link_cable_room_descriptor(
                RoomStatus::WaitingForGuest,
                2,
                &gba,
                LobbyLinkProtocolFamily::GbSerialV1,
                "mgba-link-runtime-v1",
            )
            .is_err()
        );

        let mut controller = gba.clone();
        controller.mode = NetplaySessionMode::ControllerNetplay;
        controller.link = None;
        assert!(
            validate_link_cable_room_descriptor(
                RoomStatus::WaitingForGuest,
                2,
                &controller,
                LobbyLinkProtocolFamily::GbaMultiV2,
                "mgba-link-runtime-v1",
            )
            .is_err()
        );
    }

    #[test]
    fn lobby_view_for_legacy_client_downgrades_playing_launch_status() {
        let playing = lobby_view_with_playing_launch();
        let legacy = lobby_view_for_client(playing.clone(), &LobbyClientCapabilities::default());

        let legacy_launch = legacy.pending_launch.expect("legacy launch");
        assert_eq!(legacy_launch.status, LobbyGameLaunchStatus::Ready);
        assert!(legacy_launch.gameplay_started_at_ms.is_none());
        assert!(legacy_launch.started_player_indexes.is_empty());

        let modern = lobby_view_for_client(
            playing,
            &LobbyClientCapabilities {
                supports_lobby: true,
                supports_temporary_session_rom_relay: true,
                supports_lobby_voice: true,
                supports_multi_game_lobby: true,
                supports_lobby_returned_event: true,
                supports_lobby_gameplay_started: true,
                supports_lobby_player_removed_event: true,
                link_cable: None,
            },
        );
        let modern_launch = modern.pending_launch.expect("modern launch");
        assert_eq!(modern_launch.status, LobbyGameLaunchStatus::Playing);
        assert_eq!(modern_launch.gameplay_started_at_ms, Some(150));
        assert_eq!(modern_launch.started_player_indexes, vec![0, 1]);
    }

    #[test]
    fn lobby_view_only_exposes_link_extension_to_negotiated_clients() {
        let mut view = lobby_view_with_playing_launch();
        view.multiplayer_extension = Some(LobbyMultiplayerExtension {
            session_kind: LobbyMultiplayerSessionKind::LinkCable,
            generation: 1,
            link_cable: Some(LobbyLinkCableView {
                protocol_family: LobbyLinkProtocolFamily::GbaMultiV2,
                max_players: 2,
                room_invite_code: Some("EF45-GH".to_string()),
                cable_epoch: None,
                players: vec![LobbyLinkCablePlayerSlotView {
                    player_index: 0,
                    cable_slot: 0,
                    selection_generation: 1,
                    selected_game: None,
                    launch_state: LobbyLinkCableLaunchState::NotLaunched,
                    updated_at_ms: 100,
                }],
            }),
        });

        let legacy = lobby_view_for_client(view.clone(), &LobbyClientCapabilities::default());
        assert!(legacy.multiplayer_extension.is_none());

        let capable = lobby_view_for_client(
            view,
            &LobbyClientCapabilities {
                link_cable: Some(LobbyLinkCableClientCapabilities {
                    contract_version: 1,
                    runtime_profile: "mgba-link-runtime-v1".to_string(),
                    core_build_id: "android-mgba-link-v1".to_string(),
                    protocol_families: vec![LobbyLinkProtocolFamily::GbaMultiV2],
                }),
                ..LobbyClientCapabilities::default()
            },
        );
        assert!(capable.multiplayer_extension.is_some());
    }

    fn lobby_view_with_playing_launch() -> LobbyView {
        LobbyView {
            lobby_id: RoomId::new(),
            event_seq: 5,
            lobby_epoch: 4,
            invite_code: "AB23-CD".to_owned(),
            created_at_ms: 1,
            updated_at_ms: 150,
            last_meaningful_activity_at_ms: 150,
            status: LobbyStatus::InGame,
            visibility: LobbyVisibility::Private,
            capabilities: LobbyServerCapabilities::current(MAX_LOBBY_PLAYERS, true, true),
            players: Vec::new(),
            selected_game: None,
            game_readiness: Vec::new(),
            pending_launch: Some(LobbyGameLaunchView {
                proposal_id: uuid::Uuid::new_v4(),
                requested_by_player_index: PlayerIndex::ONE.zero_based(),
                requested_at_ms: 100,
                status: LobbyGameLaunchStatus::Playing,
                room_invite_code: Some("ROOM-1".to_owned()),
                room_published_at_ms: Some(120),
                gameplay_started_at_ms: Some(150),
                started_player_indexes: vec![0, 1],
            }),
            voice: None,
            multiplayer_extension: None,
        }
    }

    fn link_session(
        game_system: &str,
        system_family: &str,
        link_protocol: &str,
    ) -> NetplaySessionDescriptor {
        serde_json::from_value(json!({
            "mode": "linkCable",
            "game": {
                "systemId": game_system,
                "title": "Link Game",
                "romSha256": "a".repeat(64),
                "contentKey": "link-game"
            },
            "core": {
                "coreId": "mgba"
            },
            "link": {
                "systemFamily": system_family,
                "linkProtocol": link_protocol,
                "runtimeProfile": "mgba-link-runtime-v1",
                "maxPlayers": 2,
                "transport": "relay"
            }
        }))
        .expect("link session descriptor")
    }
}
