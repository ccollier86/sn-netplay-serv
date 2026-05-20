//! Compact binary server-frame release messages.
//!
//! Input batches and server-frame releases share the dedicated binary input
//! socket. The relay sends one server-frame release at a time so clients can
//! pace prediction, stalls, and reconnect recovery against a single canonical
//! frame cursor.

use crate::limits::MAX_INPUT_BATCH_BYTES;

const SERVER_FRAME_MAGIC: &[u8; 4] = b"SBF1";
const SERVER_FRAME_TYPE: u8 = 2;
const SERVER_FRAME_BYTES: usize = 4 + 1 + 8 + 8 + 8 + 8;

/// Canonical controller frame released by the relay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerFrame {
    /// Room epoch observed by the relay when the frame was released.
    pub room_epoch: u64,
    /// Session epoch observed by the relay when the frame was released.
    pub session_epoch: u64,
    /// Exact frame clients may now treat as relay-released.
    pub frame: u64,
    /// Latest canonical room frame known at release time.
    pub canonical_frame: u64,
}

/// Binary server-frame codec failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ServerFrameCodecError {
    /// Message was empty, too large, or missing required bytes.
    #[error("server frame is malformed")]
    Malformed,
    /// Message magic or type did not match this codec.
    #[error("server frame type is unsupported")]
    Unsupported,
}

/// Decodes one binary server-frame release message.
pub fn decode_server_frame(payload: &[u8]) -> Result<ServerFrame, ServerFrameCodecError> {
    if payload.len() != SERVER_FRAME_BYTES || payload.len() > MAX_INPUT_BATCH_BYTES {
        return Err(ServerFrameCodecError::Malformed);
    }

    if &payload[0..4] != SERVER_FRAME_MAGIC || payload[4] != SERVER_FRAME_TYPE {
        return Err(ServerFrameCodecError::Unsupported);
    }

    Ok(ServerFrame {
        room_epoch: read_u64(payload, 5)?,
        session_epoch: read_u64(payload, 13)?,
        frame: read_u64(payload, 21)?,
        canonical_frame: read_u64(payload, 29)?,
    })
}

/// Encodes a binary server-frame release message.
pub fn encode_server_frame(frame: &ServerFrame) -> Vec<u8> {
    let mut payload = Vec::with_capacity(SERVER_FRAME_BYTES);

    payload.extend_from_slice(SERVER_FRAME_MAGIC);
    payload.push(SERVER_FRAME_TYPE);
    payload.extend_from_slice(&frame.room_epoch.to_be_bytes());
    payload.extend_from_slice(&frame.session_epoch.to_be_bytes());
    payload.extend_from_slice(&frame.frame.to_be_bytes());
    payload.extend_from_slice(&frame.canonical_frame.to_be_bytes());

    payload
}

fn read_u64(payload: &[u8], offset: usize) -> Result<u64, ServerFrameCodecError> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or(ServerFrameCodecError::Malformed)?;
    Ok(u64::from_be_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

#[cfg(test)]
mod tests {
    use super::{ServerFrame, decode_server_frame, encode_server_frame};

    #[test]
    fn round_trips_server_frame() {
        let frame = ServerFrame {
            room_epoch: 2,
            session_epoch: 3,
            frame: 10,
            canonical_frame: 12,
        };

        assert_eq!(decode_server_frame(&encode_server_frame(&frame)), Ok(frame));
    }
}
