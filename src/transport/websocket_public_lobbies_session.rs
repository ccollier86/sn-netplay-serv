//! WebSocket session loop for the public lobby directory.
//!
//! This socket broadcasts safe public lobby summaries only. It never exposes
//! full lobby views, content hashes, resume tokens, relay grants, or license
//! details.

use crate::http::AppServices;
use crate::lobbies::PublicLobbySummary;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;

/// Handles one upgraded public lobby directory WebSocket.
pub async fn handle_public_lobbies_websocket_session(socket: WebSocket, services: AppServices) {
    let mut public_events = services.lobbies.subscribe_public_lobbies().await;
    let (mut sender, mut receiver) = socket.split();

    if send_public_lobbies_snapshot(&mut sender, &services)
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                if !handle_public_lobbies_incoming(incoming).await {
                    break;
                }
            }
            event = public_events.recv() => {
                match event {
                    Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if send_public_lobbies_snapshot(&mut sender, &services).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn handle_public_lobbies_incoming(incoming: Option<Result<Message, axum::Error>>) -> bool {
    match incoming {
        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => false,
        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => true,
        Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) => true,
    }
}

async fn send_public_lobbies_snapshot(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    services: &AppServices,
) -> Result<(), axum::Error> {
    let message = PublicLobbyDirectoryServerMessage::Snapshot {
        lobbies: services.lobbies.public_lobbies().await,
    };
    let payload = serde_json::to_string(&message).expect("public lobby snapshot serializes");

    sender.send(Message::Text(payload.into())).await
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum PublicLobbyDirectoryServerMessage {
    Snapshot {
        #[serde(rename = "lobbies")]
        lobbies: Vec<PublicLobbySummary>,
    },
}
