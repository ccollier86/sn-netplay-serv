//! Cumulative protocol v5 input acceptance responses.

use super::StrictInputCodecError;
use super::codec_common::{
    COMMON_EPOCH_HEADER_BYTES, read_player_index, read_u64, validate_exact_message_header,
    write_header,
};
use crate::rooms::PlayerIndex;

const ACK_MAGIC: &[u8; 4] = b"SBA1";
const ACK_TYPE: u8 = 4;
const NACK_MAGIC: &[u8; 4] = b"SBN1";
const NACK_TYPE: u8 = 5;
const ACK_BYTES: usize = COMMON_EPOCH_HEADER_BYTES + 1 + 8;
const NACK_BYTES: usize = COMMON_EPOCH_HEADER_BYTES + 1 + 8 + 8 + 1;

/// Cumulative server acceptance response for one player's input lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputCursorAck {
    pub room_epoch: u64,
    pub session_epoch: u64,
    pub player_index: PlayerIndex,
    /// First frame the server has not accepted from this player.
    pub next_expected_frame: u64,
}

/// Stable reason carried by a cumulative input rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum InputCursorNackReason {
    InputGap = 1,
    FutureFrameTooLarge = 2,
    SessionState = 3,
}

impl TryFrom<u8> for InputCursorNackReason {
    type Error = StrictInputCodecError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::InputGap),
            2 => Ok(Self::FutureFrameTooLarge),
            3 => Ok(Self::SessionState),
            _ => Err(StrictInputCodecError::InvalidNackReason),
        }
    }
}

/// Cumulative server rejection for a recoverable input cursor mismatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputCursorNack {
    pub room_epoch: u64,
    pub session_epoch: u64,
    pub player_index: PlayerIndex,
    pub expected_frame: u64,
    pub received_frame: u64,
    pub reason: InputCursorNackReason,
}

/// Server response to one shape-valid strict input batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputCursorResponse {
    /// Batch was accepted or contained only old duplicates.
    Ack(InputCursorAck),
    /// Batch could not advance from the exact expected cursor.
    Nack(InputCursorNack),
}

pub fn encode_input_cursor_ack(ack: &InputCursorAck) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(ACK_BYTES);
    write_header(
        &mut encoded,
        ACK_MAGIC,
        ACK_TYPE,
        ack.room_epoch,
        ack.session_epoch,
    );
    encoded.push(ack.player_index.zero_based());
    encoded.extend_from_slice(&ack.next_expected_frame.to_be_bytes());
    encoded
}

pub fn decode_input_cursor_ack(payload: &[u8]) -> Result<InputCursorAck, StrictInputCodecError> {
    validate_exact_message_header(payload, ACK_BYTES, ACK_MAGIC, ACK_TYPE)?;
    Ok(InputCursorAck {
        room_epoch: read_u64(payload, 5)?,
        session_epoch: read_u64(payload, 13)?,
        player_index: read_player_index(payload, 21)?,
        next_expected_frame: read_u64(payload, 22)?,
    })
}

pub fn encode_input_cursor_nack(nack: &InputCursorNack) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(NACK_BYTES);
    write_header(
        &mut encoded,
        NACK_MAGIC,
        NACK_TYPE,
        nack.room_epoch,
        nack.session_epoch,
    );
    encoded.push(nack.player_index.zero_based());
    encoded.extend_from_slice(&nack.expected_frame.to_be_bytes());
    encoded.extend_from_slice(&nack.received_frame.to_be_bytes());
    encoded.push(nack.reason as u8);
    encoded
}

pub fn decode_input_cursor_nack(payload: &[u8]) -> Result<InputCursorNack, StrictInputCodecError> {
    validate_exact_message_header(payload, NACK_BYTES, NACK_MAGIC, NACK_TYPE)?;
    Ok(InputCursorNack {
        room_epoch: read_u64(payload, 5)?,
        session_epoch: read_u64(payload, 13)?,
        player_index: read_player_index(payload, 21)?,
        expected_frame: read_u64(payload, 22)?,
        received_frame: read_u64(payload, 30)?,
        reason: InputCursorNackReason::try_from(payload[38])?,
    })
}
