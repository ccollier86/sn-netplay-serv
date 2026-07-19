//! Protocol v5 binary input-lane request handling.

use crate::http::AppServices;
use crate::protocol::{
    InputCursorResponse, decode_host_frame_open, decode_strict_input_batch,
    encode_input_cursor_ack, encode_input_cursor_nack, encode_server_frame_release_v5,
};
use crate::rooms::{ConnectionId, HostFrameRelayOutcome, InviteCode};
use crate::transport::websocket_outbound::{SocketSender, send_binary_message};

/// Applies one protocol v5 binary message without emitting JSON on the hot lane.
pub(super) async fn handle_v5_binary_message(
    sender: &mut SocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    payload: &[u8],
) -> bool {
    if payload.starts_with(b"SBI3") {
        return handle_strict_input(sender, services, invite_code, connection_id, payload).await;
    }
    if payload.starts_with(b"SBO1") {
        return handle_host_open(sender, services, invite_code, connection_id, payload).await;
    }

    reject_v5_message(
        services,
        invite_code,
        connection_id,
        "relay rejected unsupported v5 input-lane message".to_string(),
    )
    .await;
    false
}

async fn handle_strict_input(
    sender: &mut SocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    payload: &[u8],
) -> bool {
    let batch = match decode_strict_input_batch(payload) {
        Ok(batch) => batch,
        Err(error) => {
            reject_v5_message(
                services,
                invite_code,
                connection_id,
                format!("relay rejected strict input: {error}"),
            )
            .await;
            return false;
        }
    };
    let response = match services
        .rooms
        .relay_strict_input_batch(invite_code.clone(), connection_id, batch)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            reject_v5_message(
                services,
                invite_code,
                connection_id,
                format!("relay rejected strict input: {error}"),
            )
            .await;
            return false;
        }
    };
    let encoded = match response {
        InputCursorResponse::Ack(ack) => encode_input_cursor_ack(&ack),
        InputCursorResponse::Nack(nack) => encode_input_cursor_nack(&nack),
    };
    send_binary_message(sender, encoded).await.is_ok()
}

async fn handle_host_open(
    sender: &mut SocketSender,
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    payload: &[u8],
) -> bool {
    let open = match decode_host_frame_open(payload) {
        Ok(open) => open,
        Err(error) => {
            reject_v5_message(
                services,
                invite_code,
                connection_id,
                format!("relay rejected host frame open: {error}"),
            )
            .await;
            return false;
        }
    };
    match services
        .rooms
        .relay_host_frame_open(invite_code.clone(), connection_id, open)
        .await
    {
        Ok(HostFrameRelayOutcome::Duplicate(duplicate_release)) => {
            let encoded = match encode_server_frame_release_v5(&duplicate_release) {
                Ok(encoded) => encoded,
                Err(_) => return false,
            };
            send_binary_message(sender, encoded).await.is_ok()
        }
        Ok(HostFrameRelayOutcome::Broadcast) => true,
        Ok(HostFrameRelayOutcome::Pending {
            delay_ms,
            room_epoch,
            session_epoch,
            frame,
        }) => {
            schedule_first_frame_release(
                services,
                invite_code.clone(),
                delay_ms,
                room_epoch,
                session_epoch,
                frame,
            );
            true
        }
        Err(error) => {
            reject_v5_message(
                services,
                invite_code,
                connection_id,
                format!("relay rejected host frame open: {error}"),
            )
            .await;
            false
        }
    }
}

fn schedule_first_frame_release(
    services: &AppServices,
    invite_code: InviteCode,
    delay_ms: u64,
    room_epoch: u64,
    session_epoch: u64,
    frame: u64,
) {
    let rooms = services.rooms.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        let _ = rooms
            .release_scheduled_v5_host_frame(invite_code, room_epoch, session_epoch, frame)
            .await;
    });
}

async fn reject_v5_message(
    services: &AppServices,
    invite_code: &InviteCode,
    connection_id: ConnectionId,
    reason: String,
) {
    services.metrics.record_protocol_error();
    let _ = services
        .rooms
        .record_transport_close(invite_code.clone(), connection_id, "input", reason)
        .await;
}
