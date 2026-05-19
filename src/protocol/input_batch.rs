//! Compact binary controller input batches.
//!
//! Control messages stay JSON. This codec is reserved for the high-frequency
//! gameplay input channel so relay parsing and forwarding stay small.

use crate::limits::{MAX_INPUT_BATCH_BYTES, MAX_INPUT_BATCH_FRAMES};
use crate::protocol::InputFrame;
use crate::rooms::PlayerIndex;

const INPUT_BATCH_MAGIC: &[u8; 4] = b"SBI1";
const INPUT_BATCH_TYPE: u8 = 1;
const INPUT_BATCH_HEADER_BYTES: usize = 4 + 1 + 8 + 8 + 1 + 1;
const INPUT_FRAME_HEADER_BYTES: usize = 8 + 2;

/// Binary input-batch payload after decoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputFrameBatch {
    /// Room epoch observed by the sender.
    pub room_epoch: u64,
    /// Session epoch observed by the sender.
    pub session_epoch: u64,
    /// Player slot that owns every frame in the batch.
    pub player_index: PlayerIndex,
    /// Ordered input frames.
    pub frames: Vec<InputFrame>,
}

/// Binary input-batch codec failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InputFrameBatchCodecError {
    /// Message was empty, too large, or missing required bytes.
    #[error("input batch is malformed")]
    Malformed,
    /// Message magic or type did not match this codec.
    #[error("input batch type is unsupported")]
    Unsupported,
    /// Message contained too many frames.
    #[error("input batch contains too many frames")]
    TooManyFrames,
    /// Message supplied an invalid player index.
    #[error("input batch player index is invalid")]
    InvalidPlayerIndex,
    /// Message did not contain any frames.
    #[error("input batch is empty")]
    Empty,
}

/// Decodes one binary input-batch message.
pub fn decode_input_frame_batch(
    payload: &[u8],
) -> Result<InputFrameBatch, InputFrameBatchCodecError> {
    if payload.len() > MAX_INPUT_BATCH_BYTES || payload.len() < INPUT_BATCH_HEADER_BYTES {
        return Err(InputFrameBatchCodecError::Malformed);
    }

    if &payload[0..4] != INPUT_BATCH_MAGIC || payload[4] != INPUT_BATCH_TYPE {
        return Err(InputFrameBatchCodecError::Unsupported);
    }

    let room_epoch = read_u64(payload, 5)?;
    let session_epoch = read_u64(payload, 13)?;
    let player_index = PlayerIndex::new(payload[21], crate::limits::MVP_ROOM_CAPACITY)
        .ok_or(InputFrameBatchCodecError::InvalidPlayerIndex)?;
    let frame_count = usize::from(payload[22]);

    if frame_count == 0 {
        return Err(InputFrameBatchCodecError::Empty);
    }

    if frame_count > MAX_INPUT_BATCH_FRAMES {
        return Err(InputFrameBatchCodecError::TooManyFrames);
    }

    let mut offset = INPUT_BATCH_HEADER_BYTES;
    let mut frames = Vec::with_capacity(frame_count);

    for _ in 0..frame_count {
        if payload.len().saturating_sub(offset) < INPUT_FRAME_HEADER_BYTES {
            return Err(InputFrameBatchCodecError::Malformed);
        }

        let frame = read_u64(payload, offset)?;
        offset += 8;
        let payload_len = usize::from(read_u16(payload, offset)?);
        offset += 2;

        if payload.len().saturating_sub(offset) < payload_len {
            return Err(InputFrameBatchCodecError::Malformed);
        }

        frames.push(InputFrame {
            frame,
            payload: payload[offset..offset + payload_len].to_vec(),
            player_index,
        });
        offset += payload_len;
    }

    if offset != payload.len() {
        return Err(InputFrameBatchCodecError::Malformed);
    }

    Ok(InputFrameBatch {
        frames,
        player_index,
        room_epoch,
        session_epoch,
    })
}

/// Encodes a binary input-batch message for relay fanout.
pub fn encode_input_frame_batch(
    batch: &InputFrameBatch,
) -> Result<Vec<u8>, InputFrameBatchCodecError> {
    if batch.frames.is_empty() {
        return Err(InputFrameBatchCodecError::Empty);
    }

    if batch.frames.len() > MAX_INPUT_BATCH_FRAMES {
        return Err(InputFrameBatchCodecError::TooManyFrames);
    }

    let mut payload = Vec::with_capacity(INPUT_BATCH_HEADER_BYTES);
    payload.extend_from_slice(INPUT_BATCH_MAGIC);
    payload.push(INPUT_BATCH_TYPE);
    payload.extend_from_slice(&batch.room_epoch.to_be_bytes());
    payload.extend_from_slice(&batch.session_epoch.to_be_bytes());
    payload.push(batch.player_index.zero_based());
    payload.push(
        u8::try_from(batch.frames.len()).map_err(|_| InputFrameBatchCodecError::TooManyFrames)?,
    );

    for frame in &batch.frames {
        if frame.player_index != batch.player_index || frame.payload.len() > u16::MAX as usize {
            return Err(InputFrameBatchCodecError::Malformed);
        }

        payload.extend_from_slice(&frame.frame.to_be_bytes());
        payload.extend_from_slice(&(frame.payload.len() as u16).to_be_bytes());
        payload.extend_from_slice(&frame.payload);
    }

    if payload.len() > MAX_INPUT_BATCH_BYTES {
        return Err(InputFrameBatchCodecError::Malformed);
    }

    Ok(payload)
}

fn read_u64(payload: &[u8], offset: usize) -> Result<u64, InputFrameBatchCodecError> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or(InputFrameBatchCodecError::Malformed)?;
    Ok(u64::from_be_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

fn read_u16(payload: &[u8], offset: usize) -> Result<u16, InputFrameBatchCodecError> {
    let bytes = payload
        .get(offset..offset + 2)
        .ok_or(InputFrameBatchCodecError::Malformed)?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

#[cfg(test)]
mod tests {
    use super::{InputFrameBatch, decode_input_frame_batch, encode_input_frame_batch};
    use crate::protocol::InputFrame;
    use crate::rooms::PlayerIndex;

    #[test]
    fn round_trips_input_batch() {
        let batch = InputFrameBatch {
            frames: vec![
                InputFrame {
                    frame: 10,
                    payload: vec![1, 2],
                    player_index: PlayerIndex::ONE,
                },
                InputFrame {
                    frame: 11,
                    payload: vec![3, 4],
                    player_index: PlayerIndex::ONE,
                },
            ],
            player_index: PlayerIndex::ONE,
            room_epoch: 2,
            session_epoch: 3,
        };

        let encoded = encode_input_frame_batch(&batch).expect("encoded");
        let decoded = decode_input_frame_batch(&encoded).expect("decoded");

        assert_eq!(decoded, batch);
    }
}
