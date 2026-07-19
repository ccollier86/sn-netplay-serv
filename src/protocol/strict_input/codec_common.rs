//! Shared byte-level helpers for protocol v5 input-lane messages.

use super::StrictInputCodecError;
use crate::rooms::PlayerIndex;

pub(super) const COMMON_EPOCH_HEADER_BYTES: usize = 4 + 1 + 8 + 8;

pub(super) fn write_header(
    encoded: &mut Vec<u8>,
    magic: &[u8; 4],
    message_type: u8,
    room_epoch: u64,
    session_epoch: u64,
) {
    encoded.extend_from_slice(magic);
    encoded.push(message_type);
    encoded.extend_from_slice(&room_epoch.to_be_bytes());
    encoded.extend_from_slice(&session_epoch.to_be_bytes());
}

pub(super) fn validate_exact_message_header(
    payload: &[u8],
    expected_bytes: usize,
    magic: &[u8; 4],
    message_type: u8,
) -> Result<(), StrictInputCodecError> {
    if payload.len() != expected_bytes {
        return Err(StrictInputCodecError::Malformed);
    }
    validate_message_header(payload, expected_bytes, magic, message_type)
}

pub(super) fn validate_message_header(
    payload: &[u8],
    minimum_bytes: usize,
    magic: &[u8; 4],
    message_type: u8,
) -> Result<(), StrictInputCodecError> {
    if payload.len() < minimum_bytes {
        return Err(StrictInputCodecError::Malformed);
    }
    if &payload[0..4] != magic || payload[4] != message_type {
        return Err(StrictInputCodecError::Unsupported);
    }
    Ok(())
}

pub(super) fn read_player_index(
    payload: &[u8],
    offset: usize,
) -> Result<PlayerIndex, StrictInputCodecError> {
    PlayerIndex::new(
        *payload
            .get(offset)
            .ok_or(StrictInputCodecError::Malformed)?,
        crate::limits::MVP_ROOM_CAPACITY,
    )
    .ok_or(StrictInputCodecError::InvalidPlayerIndex)
}

pub(super) fn read_u64(payload: &[u8], offset: usize) -> Result<u64, StrictInputCodecError> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or(StrictInputCodecError::Malformed)?;
    Ok(u64::from_be_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

pub(super) fn read_u16(payload: &[u8], offset: usize) -> Result<u16, StrictInputCodecError> {
    let bytes = payload
        .get(offset..offset + 2)
        .ok_or(StrictInputCodecError::Malformed)?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}
