//! Zero-copy binary controller input records.
//!
//! `SBI2` is an optional v2 input format for clients that advertise fast input
//! relay support. Each record is self-contained so the server can validate it
//! and forward the same bytes to peers without re-encoding.

use crate::limits::{MAX_INPUT_BATCH_BYTES, MAX_INPUT_BATCH_FRAMES};
use crate::rooms::PlayerIndex;
use bytes::Bytes;

const FAST_INPUT_MAGIC: &[u8; 4] = b"SBI2";
const FAST_INPUT_TYPE: u8 = 2;
const FAST_INPUT_RECORD_HEADER_BYTES: usize = 4 + 1 + 8 + 8 + 1 + 8 + 2;

/// Decoded fast-input records from one binary WebSocket message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FastInputBatch {
    /// Ordered input records contained by the message.
    pub frames: Vec<FastInputFrame>,
}

/// One self-contained fast-input record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FastInputFrame {
    /// Room epoch observed by the sender.
    pub room_epoch: u64,
    /// Session epoch observed by the sender.
    pub session_epoch: u64,
    /// Player slot that owns this input frame.
    pub player_index: PlayerIndex,
    /// Canonical emulation frame number.
    pub frame: u64,
    /// Opaque controller payload.
    pub payload: Bytes,
    encoded: Bytes,
}

impl FastInputFrame {
    /// Returns the exact self-contained `SBI2` record received from the client.
    pub fn encoded(&self) -> Bytes {
        self.encoded.clone()
    }
}

/// Fast-input codec failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FastInputCodecError {
    /// Message was empty, too large, or missing required bytes.
    #[error("fast input is malformed")]
    Malformed,
    /// Message magic or type did not match this codec.
    #[error("fast input type is unsupported")]
    Unsupported,
    /// Message contained too many records.
    #[error("fast input contains too many records")]
    TooManyFrames,
    /// Message supplied an invalid player index.
    #[error("fast input player index is invalid")]
    InvalidPlayerIndex,
    /// Message did not contain any records.
    #[error("fast input is empty")]
    Empty,
}

/// Decodes one or more concatenated `SBI2` input records.
pub fn decode_fast_input_batch(payload: Bytes) -> Result<FastInputBatch, FastInputCodecError> {
    if payload.len() > MAX_INPUT_BATCH_BYTES {
        return Err(FastInputCodecError::Malformed);
    }

    let mut offset = 0;
    let mut frames = Vec::new();

    while offset < payload.len() {
        if payload.len().saturating_sub(offset) < FAST_INPUT_RECORD_HEADER_BYTES {
            return Err(FastInputCodecError::Malformed);
        }

        if payload.get(offset..offset + 4) != Some(FAST_INPUT_MAGIC.as_slice())
            || payload[offset + 4] != FAST_INPUT_TYPE
        {
            return Err(FastInputCodecError::Unsupported);
        }

        if frames.len() >= MAX_INPUT_BATCH_FRAMES {
            return Err(FastInputCodecError::TooManyFrames);
        }

        let record_start = offset;
        let room_epoch = read_u64(&payload, offset + 5)?;
        let session_epoch = read_u64(&payload, offset + 13)?;
        let player_index = PlayerIndex::new(payload[offset + 21], crate::limits::MVP_ROOM_CAPACITY)
            .ok_or(FastInputCodecError::InvalidPlayerIndex)?;
        let frame = read_u64(&payload, offset + 22)?;
        let payload_len = usize::from(read_u16(&payload, offset + 30)?);
        offset += FAST_INPUT_RECORD_HEADER_BYTES;

        if payload.len().saturating_sub(offset) < payload_len {
            return Err(FastInputCodecError::Malformed);
        }

        let payload_end = offset + payload_len;
        frames.push(FastInputFrame {
            room_epoch,
            session_epoch,
            player_index,
            frame,
            payload: payload.slice(offset..payload_end),
            encoded: payload.slice(record_start..payload_end),
        });
        offset = payload_end;
    }

    if frames.is_empty() {
        return Err(FastInputCodecError::Empty);
    }

    Ok(FastInputBatch { frames })
}

/// Encodes one self-contained `SBI2` input record.
pub fn encode_fast_input_frame(
    room_epoch: u64,
    session_epoch: u64,
    player_index: PlayerIndex,
    frame: u64,
    input_payload: &[u8],
) -> Result<Bytes, FastInputCodecError> {
    if input_payload.len() > u16::MAX as usize {
        return Err(FastInputCodecError::Malformed);
    }

    let mut payload = Vec::with_capacity(FAST_INPUT_RECORD_HEADER_BYTES + input_payload.len());
    payload.extend_from_slice(FAST_INPUT_MAGIC);
    payload.push(FAST_INPUT_TYPE);
    payload.extend_from_slice(&room_epoch.to_be_bytes());
    payload.extend_from_slice(&session_epoch.to_be_bytes());
    payload.push(player_index.zero_based());
    payload.extend_from_slice(&frame.to_be_bytes());
    payload.extend_from_slice(&(input_payload.len() as u16).to_be_bytes());
    payload.extend_from_slice(input_payload);

    if payload.len() > MAX_INPUT_BATCH_BYTES {
        return Err(FastInputCodecError::Malformed);
    }

    Ok(Bytes::from(payload))
}

fn read_u64(payload: &[u8], offset: usize) -> Result<u64, FastInputCodecError> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or(FastInputCodecError::Malformed)?;
    Ok(u64::from_be_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

fn read_u16(payload: &[u8], offset: usize) -> Result<u16, FastInputCodecError> {
    let bytes = payload
        .get(offset..offset + 2)
        .ok_or(FastInputCodecError::Malformed)?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

#[cfg(test)]
mod tests {
    use super::{decode_fast_input_batch, encode_fast_input_frame};
    use crate::rooms::PlayerIndex;
    use bytes::{BufMut, BytesMut};

    #[test]
    fn decodes_concatenated_records_without_reencoding() {
        let first =
            encode_fast_input_frame(2, 3, PlayerIndex::ONE, 10, &[1, 2]).expect("first frame");
        let second =
            encode_fast_input_frame(2, 3, PlayerIndex::ONE, 11, &[3, 4]).expect("second frame");
        let mut payload = BytesMut::new();
        payload.put(first.clone());
        payload.put(second.clone());

        let decoded = decode_fast_input_batch(payload.freeze()).expect("decoded");

        assert_eq!(decoded.frames.len(), 2);
        assert_eq!(decoded.frames[0].encoded(), first);
        assert_eq!(decoded.frames[1].encoded(), second);
        assert_eq!(decoded.frames[0].payload.as_ref(), &[1, 2]);
        assert_eq!(decoded.frames[1].payload.as_ref(), &[3, 4]);
    }
}
