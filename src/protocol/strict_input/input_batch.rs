//! Contiguous fixed-width protocol v5 controller input batches.

use super::StrictInputCodecError;
use super::codec_common::{
    COMMON_EPOCH_HEADER_BYTES, read_player_index, read_u16, read_u64, validate_message_header,
    write_header,
};
use crate::limits::{MAX_INPUT_BATCH_BYTES, MAX_INPUT_BATCH_FRAMES, V5_RETROPAD_INPUT_BYTES};
use crate::rooms::PlayerIndex;

const MAGIC: &[u8; 4] = b"SBI3";
const MESSAGE_TYPE: u8 = 3;
const HEADER_BYTES: usize = COMMON_EPOCH_HEADER_BYTES + 1 + 1 + 2 + 8;

/// Canonical `shadowboy-retropad-v1-le` input bytes.
pub type RetropadInputPayload = [u8; V5_RETROPAD_INPUT_BYTES];

/// Contiguous protocol v5 controller input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictInputBatch {
    /// Room ownership epoch.
    pub room_epoch: u64,
    /// Deterministic gameplay epoch.
    pub session_epoch: u64,
    /// Player that owns every input in this batch.
    pub player_index: PlayerIndex,
    /// Frame represented by the first payload.
    pub start_frame: u64,
    /// Consecutive fixed-size payloads beginning at `start_frame`.
    pub payloads: Vec<RetropadInputPayload>,
}

impl StrictInputBatch {
    /// Inclusive frame represented by the last payload.
    pub fn end_frame(&self) -> u64 {
        self.start_frame
            .saturating_add(self.payloads.len().saturating_sub(1) as u64)
    }
}

/// Encodes one contiguous protocol v5 input batch.
pub fn encode_strict_input_batch(
    batch: &StrictInputBatch,
) -> Result<Vec<u8>, StrictInputCodecError> {
    validate_payload_count(batch.payloads.len())?;
    let payload_bytes = batch
        .payloads
        .len()
        .checked_mul(V5_RETROPAD_INPUT_BYTES)
        .ok_or(StrictInputCodecError::Malformed)?;
    let total_bytes = HEADER_BYTES
        .checked_add(payload_bytes)
        .ok_or(StrictInputCodecError::Malformed)?;
    if total_bytes > MAX_INPUT_BATCH_BYTES {
        return Err(StrictInputCodecError::Malformed);
    }

    let mut encoded = Vec::with_capacity(total_bytes);
    write_header(
        &mut encoded,
        MAGIC,
        MESSAGE_TYPE,
        batch.room_epoch,
        batch.session_epoch,
    );
    encoded.push(batch.player_index.zero_based());
    encoded.push(batch.payloads.len() as u8);
    encoded.extend_from_slice(&(V5_RETROPAD_INPUT_BYTES as u16).to_be_bytes());
    encoded.extend_from_slice(&batch.start_frame.to_be_bytes());
    for payload in &batch.payloads {
        encoded.extend_from_slice(payload);
    }
    Ok(encoded)
}

/// Decodes and shape-validates one contiguous protocol v5 input batch.
pub fn decode_strict_input_batch(
    payload: &[u8],
) -> Result<StrictInputBatch, StrictInputCodecError> {
    validate_message_header(payload, HEADER_BYTES, MAGIC, MESSAGE_TYPE)?;
    if payload.len() > MAX_INPUT_BATCH_BYTES {
        return Err(StrictInputCodecError::Malformed);
    }

    let frame_count = usize::from(payload[22]);
    validate_payload_count(frame_count)?;
    if usize::from(read_u16(payload, 23)?) != V5_RETROPAD_INPUT_BYTES {
        return Err(StrictInputCodecError::InvalidPayloadSize);
    }
    let expected_bytes = HEADER_BYTES
        .checked_add(
            frame_count
                .checked_mul(V5_RETROPAD_INPUT_BYTES)
                .ok_or(StrictInputCodecError::Malformed)?,
        )
        .ok_or(StrictInputCodecError::Malformed)?;
    if payload.len() != expected_bytes {
        return Err(StrictInputCodecError::Malformed);
    }

    let payloads = payload[HEADER_BYTES..]
        .chunks_exact(V5_RETROPAD_INPUT_BYTES)
        .map(|chunk| chunk.try_into().expect("fixed chunks"))
        .collect();
    Ok(StrictInputBatch {
        room_epoch: read_u64(payload, 5)?,
        session_epoch: read_u64(payload, 13)?,
        player_index: read_player_index(payload, 21)?,
        start_frame: read_u64(payload, 25)?,
        payloads,
    })
}

fn validate_payload_count(frame_count: usize) -> Result<(), StrictInputCodecError> {
    if frame_count == 0 {
        Err(StrictInputCodecError::Empty)
    } else if frame_count > MAX_INPUT_BATCH_FRAMES {
        Err(StrictInputCodecError::TooManyFrames)
    } else {
        Ok(())
    }
}
