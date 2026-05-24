//! Voice-specific WebSocket request helpers.
//!
//! Voice token renewal is private per socket and deliberately lives outside the
//! broad message dispatcher so optional voice support stays isolated.

use crate::http::AppServices;
use crate::protocol::ServerMessage;
use crate::rooms::{ConnectionId, InviteCode};
use crate::transport::websocket_outbound::{SocketSender, send_room_error, send_server_message};

/// Refreshes this connection's private voice token and sends it only to sender.
pub async fn handle_refresh_voice_token(
    sender: &mut SocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
) -> Result<(), axum::Error> {
    let refresh = match services
        .rooms
        .refresh_voice_token(invite_code.clone(), connection_id)
        .await
    {
        Ok(refresh) => refresh,
        Err(error) => return send_room_error(sender, error).await,
    };

    send_server_message(
        sender,
        &ServerMessage::VoiceTokenRefreshed {
            event_seq: refresh.room.event_seq,
            room_epoch: refresh.room.room_epoch,
            session_epoch: refresh.room.session_epoch,
            voice: refresh.voice,
        },
    )
    .await
}
