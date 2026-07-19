//! WebSocket message helpers for netplay integration smoke tests.
//!
//! Keeping socket message encoding and expectation loops here prevents the
//! shared test-fixture module from becoming a catch-all.

use super::SmokeClient;
use futures_util::{SinkExt, StreamExt};
use sb_netplay_serv::protocol::{
    InputFrame, InputFrameBatch, decode_input_frame_batch, encode_input_frame_batch,
};
use sb_netplay_serv::rooms::PlayerIndex;
use serde_json::{Value, json};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::tungstenite::Message;

const READ_TIMEOUT: Duration = Duration::from_secs(2);

impl SmokeClient {
    pub async fn send(&mut self, mut payload: Value) {
        self.attach_epochs(&mut payload);
        self.socket
            .send(Message::Text(payload.to_string().into()))
            .await
            .expect("send websocket message");
    }

    pub async fn next_json(&mut self) -> Value {
        let message = timeout(READ_TIMEOUT, self.socket.next())
            .await
            .expect("websocket message timed out")
            .expect("websocket message")
            .expect("websocket result");

        match message {
            Message::Text(payload) => {
                let value = serde_json::from_str(payload.as_str()).expect("json message");
                self.update_epochs(&value);
                value
            }
            other => panic!("unexpected websocket message: {other:?}"),
        }
    }

    pub async fn send_input_frame(&mut self, frame: u64, payload: Vec<u8>) {
        let player_index = PlayerIndex::new(
            self.player_index.expect("connected player index"),
            sb_netplay_serv::limits::MVP_ROOM_CAPACITY,
        )
        .expect("valid player index");
        let encoded = encode_input_frame_batch(&InputFrameBatch {
            frames: vec![InputFrame {
                frame,
                payload,
                player_index,
            }],
            player_index,
            room_epoch: self.room_epoch,
            session_epoch: self.session_epoch,
        })
        .expect("encoded input batch");
        let input_socket = self.input_socket.as_mut().expect("input socket connected");

        input_socket
            .send(Message::Binary(encoded.into()))
            .await
            .expect("send input batch");
    }

    pub async fn expect_type(&mut self, message_type: &str) -> Value {
        loop {
            let message = self.next_json().await;
            if message["type"] == message_type {
                return message;
            }
        }
    }

    pub async fn expect_error(&mut self, code: &str) -> Value {
        loop {
            let message = self.next_json().await;
            if message["type"] == "error" && message["code"] == code {
                return message;
            }
        }
    }

    pub async fn expect_input_from(&mut self, player_index: u8) -> Value {
        let input_socket = self.input_socket.as_mut().expect("input socket connected");

        loop {
            let message = timeout(READ_TIMEOUT, input_socket.next())
                .await
                .expect("input websocket message timed out")
                .expect("input websocket message")
                .expect("input websocket result");

            if let Message::Binary(payload) = message {
                let Ok(batch) = decode_input_frame_batch(&payload) else {
                    continue;
                };
                if batch.player_index.zero_based() != player_index {
                    continue;
                }
                let input = batch.frames.first().expect("input frame");

                return json!({
                    "type": "inputFrame",
                    "input": {
                        "playerIndex": input.player_index.zero_based(),
                        "frame": input.frame,
                        "payload": input.payload
                    }
                });
            }
        }
    }

    pub async fn expect_link_packet_from(&mut self, player_index: u8) -> Value {
        loop {
            let message = self.next_json().await;
            if message["type"] == "linkCablePacket"
                && message["packet"]["playerIndex"] == u64::from(player_index)
            {
                return message;
            }
        }
    }

    pub async fn expect_no_link_packet_from(&mut self, player_index: u8) {
        let result = timeout(Duration::from_millis(200), async {
            loop {
                let message = self.next_json().await;
                if message["type"] == "linkCablePacket"
                    && message["packet"]["playerIndex"] == u64::from(player_index)
                {
                    return message;
                }
            }
        })
        .await;

        if let Ok(message) = result {
            panic!("unexpected echoed link packet: {message}");
        }
    }

    fn attach_epochs(&self, payload: &mut Value) {
        let Some(object) = payload.as_object_mut() else {
            return;
        };
        let Some(message_type) = object.get("type").and_then(Value::as_str) else {
            return;
        };

        if message_type == "ping" {
            return;
        }

        object
            .entry("roomEpoch")
            .or_insert_with(|| json!(self.room_epoch));
        object
            .entry("sessionEpoch")
            .or_insert_with(|| json!(self.session_epoch));
    }

    fn update_epochs(&mut self, message: &Value) {
        if let Some(room_epoch) = message["roomEpoch"].as_u64() {
            self.room_epoch = room_epoch;
        } else if let Some(room_epoch) = message["room"]["roomEpoch"].as_u64() {
            self.room_epoch = room_epoch;
        }

        if let Some(session_epoch) = message["sessionEpoch"].as_u64() {
            self.session_epoch = session_epoch;
        } else if let Some(session_epoch) = message["room"]["sessionEpoch"].as_u64() {
            self.session_epoch = session_epoch;
        }

        if message["type"] == "roomJoined" {
            self.player_index = message["yourPlayerIndex"].as_u64().map(|value| value as u8);
        }
    }
}
