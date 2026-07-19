//! Protocol v5 binary input-lane request handling.

use crate::http::AppServices;
use crate::protocol::{
    InputCursorResponse, decode_host_frame_open, decode_strict_input_batch,
    encode_input_cursor_ack, encode_input_cursor_nack, encode_server_frame_release_v5,
};
use crate::rooms::{
    ConnectionId, HostFrameRelayOutcome, InviteCode, ScheduledHostFrameReleaseOutcome,
};
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
    let frame_count = batch.payloads.len() as u64;
    let outcome = match services
        .rooms
        .relay_strict_input_batch(invite_code.clone(), connection_id, batch)
        .await
    {
        Ok(outcome) => outcome,
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
    let nacked = matches!(outcome.response, InputCursorResponse::Nack(_));
    services.metrics.record_v5_input_batch(
        frame_count,
        outcome.accepted_frame_count as u64,
        outcome.duplicate_frame_count as u64,
        nacked,
    );
    let encoded = match outcome.response {
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
            services.metrics.record_v5_host_frame_open(true);
            let encoded = match encode_server_frame_release_v5(&duplicate_release) {
                Ok(encoded) => encoded,
                Err(_) => return false,
            };
            send_binary_message(sender, encoded).await.is_ok()
        }
        Ok(HostFrameRelayOutcome::Broadcast) => {
            services.metrics.record_v5_host_frame_open(false);
            services.metrics.record_v5_frame_released();
            true
        }
        Ok(HostFrameRelayOutcome::Pending {
            delay_ms,
            room_epoch,
            session_epoch,
            frame,
        }) => {
            services.metrics.record_v5_host_frame_open(false);
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
    let metrics = services.metrics.clone();
    tokio::spawn(async move {
        let mut remaining_ms = delay_ms.max(1);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(remaining_ms)).await;
            match rooms
                .release_scheduled_v5_host_frame(
                    invite_code.clone(),
                    room_epoch,
                    session_epoch,
                    frame,
                )
                .await
            {
                Ok(ScheduledHostFrameReleaseOutcome::RetryAfter(next_remaining_ms)) => {
                    metrics.record_v5_scheduled_wake_retry();
                    remaining_ms = next_remaining_ms.max(1);
                }
                Ok(ScheduledHostFrameReleaseOutcome::Released) => {
                    metrics.record_v5_frame_released();
                    break;
                }
                Ok(ScheduledHostFrameReleaseOutcome::Superseded) | Err(_) => break,
            }
        }
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
