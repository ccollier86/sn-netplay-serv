//! Protocol v5 host frame-open and server frame-release messages.

use super::StrictInputCodecError;
use super::codec_common::{
    COMMON_EPOCH_HEADER_BYTES, read_player_index, read_u64, validate_exact_message_header,
    validate_message_header, write_header,
};
use crate::limits::MAX_INPUT_BATCH_BYTES;
use crate::rooms::PlayerIndex;

const HOST_OPEN_MAGIC: &[u8; 4] = b"SBO1";
const HOST_OPEN_TYPE: u8 = 6;
const RELEASE_MAGIC: &[u8; 4] = b"SBF2";
const RELEASE_TYPE: u8 = 7;
const HOST_OPEN_BYTES: usize = COMMON_EPOCH_HEADER_BYTES + 8;
const RELEASE_HEADER_BYTES: usize = COMMON_EPOCH_HEADER_BYTES + 8 + 8 + 1;
const RELEASE_CURSOR_BYTES: usize = 1 + 8;

/// Host declaration that one exact core frame is ready to execute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostFrameOpen {
    pub room_epoch: u64,
    pub session_epoch: u64,
    pub frame: u64,
}

/// Cumulative accepted-input cursor included in a frame release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedInputCursor {
    pub player_index: PlayerIndex,
    pub next_expected_frame: u64,
}

/// Host-driven frame release broadcast by the protocol v5 relay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerFrameReleaseV5 {
    pub room_epoch: u64,
    pub session_epoch: u64,
    pub released_frame: u64,
    pub next_host_frame: u64,
    /// Accepted input cursors sorted by player index.
    pub accepted_inputs: Vec<AcceptedInputCursor>,
}

pub fn encode_host_frame_open(open: &HostFrameOpen) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(HOST_OPEN_BYTES);
    write_header(
        &mut encoded,
        HOST_OPEN_MAGIC,
        HOST_OPEN_TYPE,
        open.room_epoch,
        open.session_epoch,
    );
    encoded.extend_from_slice(&open.frame.to_be_bytes());
    encoded
}

pub fn decode_host_frame_open(payload: &[u8]) -> Result<HostFrameOpen, StrictInputCodecError> {
    validate_exact_message_header(payload, HOST_OPEN_BYTES, HOST_OPEN_MAGIC, HOST_OPEN_TYPE)?;
    Ok(HostFrameOpen {
        room_epoch: read_u64(payload, 5)?,
        session_epoch: read_u64(payload, 13)?,
        frame: read_u64(payload, 21)?,
    })
}

pub fn encode_server_frame_release_v5(
    release: &ServerFrameReleaseV5,
) -> Result<Vec<u8>, StrictInputCodecError> {
    validate_release_cursors(&release.accepted_inputs)?;
    let total_bytes = release_message_bytes(release.accepted_inputs.len())?;
    let mut encoded = Vec::with_capacity(total_bytes);
    write_header(
        &mut encoded,
        RELEASE_MAGIC,
        RELEASE_TYPE,
        release.room_epoch,
        release.session_epoch,
    );
    encoded.extend_from_slice(&release.released_frame.to_be_bytes());
    encoded.extend_from_slice(&release.next_host_frame.to_be_bytes());
    encoded.push(release.accepted_inputs.len() as u8);
    for cursor in &release.accepted_inputs {
        encoded.push(cursor.player_index.zero_based());
        encoded.extend_from_slice(&cursor.next_expected_frame.to_be_bytes());
    }
    Ok(encoded)
}

pub fn decode_server_frame_release_v5(
    payload: &[u8],
) -> Result<ServerFrameReleaseV5, StrictInputCodecError> {
    validate_message_header(payload, RELEASE_HEADER_BYTES, RELEASE_MAGIC, RELEASE_TYPE)?;
    let cursor_count = usize::from(payload[37]);
    if payload.len() != release_message_bytes(cursor_count)? {
        return Err(StrictInputCodecError::Malformed);
    }

    let mut accepted_inputs = Vec::with_capacity(cursor_count);
    let mut offset = RELEASE_HEADER_BYTES;
    for _ in 0..cursor_count {
        accepted_inputs.push(AcceptedInputCursor {
            player_index: read_player_index(payload, offset)?,
            next_expected_frame: read_u64(payload, offset + 1)?,
        });
        offset += RELEASE_CURSOR_BYTES;
    }
    validate_release_cursors(&accepted_inputs)?;

    Ok(ServerFrameReleaseV5 {
        room_epoch: read_u64(payload, 5)?,
        session_epoch: read_u64(payload, 13)?,
        released_frame: read_u64(payload, 21)?,
        next_host_frame: read_u64(payload, 29)?,
        accepted_inputs,
    })
}

fn release_message_bytes(cursor_count: usize) -> Result<usize, StrictInputCodecError> {
    if cursor_count > u8::MAX as usize {
        return Err(StrictInputCodecError::Malformed);
    }
    let total = RELEASE_HEADER_BYTES
        .checked_add(
            cursor_count
                .checked_mul(RELEASE_CURSOR_BYTES)
                .ok_or(StrictInputCodecError::Malformed)?,
        )
        .ok_or(StrictInputCodecError::Malformed)?;
    (total <= MAX_INPUT_BATCH_BYTES)
        .then_some(total)
        .ok_or(StrictInputCodecError::Malformed)
}

fn validate_release_cursors(cursors: &[AcceptedInputCursor]) -> Result<(), StrictInputCodecError> {
    if cursors
        .windows(2)
        .any(|pair| pair[0].player_index >= pair[1].player_index)
    {
        return Err(StrictInputCodecError::InvalidCursors);
    }
    Ok(())
}
