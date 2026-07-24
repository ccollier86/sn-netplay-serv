//! Little-endian encoder and decoder for the frozen SBLK v1 frame.

use super::validation::{validate_gb_frame, validate_gba_frame, validate_wire_frame};
use super::{
    GBA_SIO_MULTI_WIRE_MODE, GbSerialEvent, GbSerialFrame, GbaSioMultiEvent, GbaSioMultiFrame,
    LINK_CABLE_WIRE_HEADER_BYTES, LINK_CABLE_WIRE_VERSION, LinkCableAbortReason,
    LinkCableWireCodecError, LinkCableWireFrame, LinkCableWireHeader, LinkCableWireProtocol,
    MAX_LINK_CABLE_WIRE_BYTES,
};

const LINK_CABLE_MAGIC: [u8; 4] = *b"SBLK";
const RESERVED_FLAGS: u16 = 0;

const EVENT_MODE_SET: u8 = 1;
const EVENT_TRANSFER_START: u8 = 2;
const EVENT_TRANSFER_REPLY: u8 = 3;
const EVENT_TRANSFER_COMMIT: u8 = 4;
const EVENT_TRANSFER_ABORT: u8 = 5;
const EVENT_MODE_ACK: u8 = 6;
const EVENT_FINISH_ACK: u8 = 7;

const GBA_MODE_SET_BODY_BYTES: usize = 13;
const GBA_TRANSFER_START_BODY_BYTES: usize = 17;
const GBA_TRANSFER_REPLY_BODY_BYTES: usize = 14;
const GBA_TRANSFER_COMMIT_BODY_BYTES: usize = 12;
const GBA_TRANSFER_ABORT_BODY_BYTES: usize = 5;
const GBA_MODE_ACK_BODY_BYTES: usize = 16;
const GBA_FINISH_ACK_BODY_BYTES: usize = 12;

const GB_SERIAL_START_BODY_BYTES: usize = 15;
const GB_SERIAL_REPLY_BODY_BYTES: usize = 14;
const GB_SERIAL_COMMIT_BODY_BYTES: usize = 7;
const GB_SERIAL_ABORT_BODY_BYTES: usize = 6;

/// Encodes a typed frame using the body namespace carried by its Rust model.
pub fn encode_link_cable_wire_frame(
    frame: &LinkCableWireFrame,
) -> Result<Vec<u8>, LinkCableWireCodecError> {
    validate_wire_frame(frame)?;
    match frame {
        LinkCableWireFrame::GbaSioMulti(frame) => {
            encode_valid_gba_frame(LinkCableWireProtocol::GbaSioMultiV1, frame)
        }
        LinkCableWireFrame::GbaSioMultiV2(frame) => {
            encode_valid_gba_frame(LinkCableWireProtocol::GbaSioMultiV2, frame)
        }
        LinkCableWireFrame::GbSerial(frame) => encode_valid_gb_frame(frame),
    }
}

/// Decodes an SBLK frame using the authoritative out-of-band protocol namespace.
pub fn decode_link_cable_wire_frame(
    protocol: LinkCableWireProtocol,
    payload: &[u8],
) -> Result<LinkCableWireFrame, LinkCableWireCodecError> {
    match protocol {
        LinkCableWireProtocol::GbaSioMultiV1 => {
            decode_gba_sio_multi_frame(payload).map(LinkCableWireFrame::GbaSioMulti)
        }
        LinkCableWireProtocol::GbaSioMultiV2 => {
            decode_gba_sio_multi_v2_frame(payload).map(LinkCableWireFrame::GbaSioMultiV2)
        }
        LinkCableWireProtocol::GbSerialV1 => {
            decode_gb_serial_frame(payload).map(LinkCableWireFrame::GbSerial)
        }
    }
}

/// Encodes one `gba-sio-multi-v1` frame.
pub fn encode_gba_sio_multi_frame(
    frame: &GbaSioMultiFrame,
) -> Result<Vec<u8>, LinkCableWireCodecError> {
    validate_gba_frame(LinkCableWireProtocol::GbaSioMultiV1, frame)?;
    encode_valid_gba_frame(LinkCableWireProtocol::GbaSioMultiV1, frame)
}

/// Encodes one `gba-sio-multi-v2` frame.
pub fn encode_gba_sio_multi_v2_frame(
    frame: &GbaSioMultiFrame,
) -> Result<Vec<u8>, LinkCableWireCodecError> {
    validate_gba_frame(LinkCableWireProtocol::GbaSioMultiV2, frame)?;
    encode_valid_gba_frame(LinkCableWireProtocol::GbaSioMultiV2, frame)
}

/// Decodes one frame in the `gba-sio-multi-v1` body namespace.
pub fn decode_gba_sio_multi_frame(
    payload: &[u8],
) -> Result<GbaSioMultiFrame, LinkCableWireCodecError> {
    decode_gba_sio_multi_frame_for(LinkCableWireProtocol::GbaSioMultiV1, payload)
}

/// Decodes one frame in the `gba-sio-multi-v2` body namespace.
pub fn decode_gba_sio_multi_v2_frame(
    payload: &[u8],
) -> Result<GbaSioMultiFrame, LinkCableWireCodecError> {
    decode_gba_sio_multi_frame_for(LinkCableWireProtocol::GbaSioMultiV2, payload)
}

fn decode_gba_sio_multi_frame_for(
    protocol: LinkCableWireProtocol,
    payload: &[u8],
) -> Result<GbaSioMultiFrame, LinkCableWireCodecError> {
    let (header, event_kind, body) = decode_header(payload)?;
    let event = match event_kind {
        EVENT_MODE_SET => {
            require_body_length(body, GBA_MODE_SET_BODY_BYTES)?;
            GbaSioMultiEvent::ModeSet {
                mode: body[0],
                siocnt: read_u16(body, 1),
                rcnt: read_u16(body, 3),
                emulated_time: read_u64(body, 5)?,
            }
        }
        EVENT_TRANSFER_START => {
            require_body_length(body, GBA_TRANSFER_START_BODY_BYTES)?;
            if body[4] != GBA_SIO_MULTI_WIRE_MODE {
                return Err(LinkCableWireCodecError::InvalidGbaTransferStart);
            }
            GbaSioMultiEvent::TransferStart {
                transfer_id: read_u32(body, 0),
                siocnt: read_u16(body, 5),
                parent_word: read_u16(body, 7),
                emulated_time: read_u64(body, 9)?,
            }
        }
        EVENT_TRANSFER_REPLY => {
            require_body_length(body, GBA_TRANSFER_REPLY_BODY_BYTES)?;
            GbaSioMultiEvent::TransferReply {
                transfer_id: read_u32(body, 0),
                child_word: read_u16(body, 4),
                emulated_time: read_u64(body, 6)?,
            }
        }
        EVENT_TRANSFER_COMMIT => {
            require_body_length(body, GBA_TRANSFER_COMMIT_BODY_BYTES)?;
            GbaSioMultiEvent::TransferCommit {
                transfer_id: read_u32(body, 0),
                words: [
                    read_u16(body, 4),
                    read_u16(body, 6),
                    read_u16(body, 8),
                    read_u16(body, 10),
                ],
            }
        }
        EVENT_TRANSFER_ABORT => {
            require_body_length(body, GBA_TRANSFER_ABORT_BODY_BYTES)?;
            GbaSioMultiEvent::TransferAbort {
                transfer_id: read_u32(body, 0),
                reason: LinkCableAbortReason::try_from(body[4])?,
            }
        }
        EVENT_MODE_ACK if protocol == LinkCableWireProtocol::GbaSioMultiV2 => {
            require_body_length(body, GBA_MODE_ACK_BODY_BYTES)?;
            GbaSioMultiEvent::ModeAck {
                acknowledged_mode_sender_sequence: read_u64(body, 0)?,
                emulated_time: read_u64(body, 8)?,
            }
        }
        EVENT_FINISH_ACK if protocol == LinkCableWireProtocol::GbaSioMultiV2 => {
            require_body_length(body, GBA_FINISH_ACK_BODY_BYTES)?;
            GbaSioMultiEvent::FinishAck {
                transfer_id: read_u32(body, 0),
                emulated_time: read_u64(body, 4)?,
            }
        }
        _ => return Err(LinkCableWireCodecError::UnsupportedEventKind),
    };
    let frame = GbaSioMultiFrame { header, event };
    validate_gba_frame(protocol, &frame)?;
    Ok(frame)
}

/// Encodes one `gb-serial-v1` frame.
pub fn encode_gb_serial_frame(frame: &GbSerialFrame) -> Result<Vec<u8>, LinkCableWireCodecError> {
    validate_gb_frame(frame)?;
    encode_valid_gb_frame(frame)
}

/// Decodes one frame in the `gb-serial-v1` body namespace.
pub fn decode_gb_serial_frame(payload: &[u8]) -> Result<GbSerialFrame, LinkCableWireCodecError> {
    let (header, event_kind, body) = decode_header(payload)?;
    let event = match event_kind {
        EVENT_TRANSFER_START => {
            require_body_length(body, GB_SERIAL_START_BODY_BYTES)?;
            GbSerialEvent::Start {
                transfer_id: read_u32(body, 0),
                clock_owner_slot: body[4],
                sc_control: body[5],
                owner_byte: body[6],
                emulated_time: read_u64(body, 7)?,
            }
        }
        EVENT_TRANSFER_REPLY => {
            require_body_length(body, GB_SERIAL_REPLY_BODY_BYTES)?;
            GbSerialEvent::Reply {
                transfer_id: read_u32(body, 0),
                clock_owner_slot: body[4],
                responder_byte: body[5],
                emulated_time: read_u64(body, 6)?,
            }
        }
        EVENT_TRANSFER_COMMIT => {
            require_body_length(body, GB_SERIAL_COMMIT_BODY_BYTES)?;
            GbSerialEvent::Commit {
                transfer_id: read_u32(body, 0),
                clock_owner_slot: body[4],
                slot_bytes: [body[5], body[6]],
            }
        }
        EVENT_TRANSFER_ABORT => {
            require_body_length(body, GB_SERIAL_ABORT_BODY_BYTES)?;
            GbSerialEvent::Abort {
                transfer_id: read_u32(body, 0),
                clock_owner_slot: body[4],
                reason: LinkCableAbortReason::try_from(body[5])?,
            }
        }
        _ => return Err(LinkCableWireCodecError::UnsupportedEventKind),
    };
    let frame = GbSerialFrame { header, event };
    validate_gb_frame(&frame)?;
    Ok(frame)
}

fn encode_valid_gba_frame(
    protocol: LinkCableWireProtocol,
    frame: &GbaSioMultiFrame,
) -> Result<Vec<u8>, LinkCableWireCodecError> {
    let (event_kind, body_length) = match &frame.event {
        GbaSioMultiEvent::ModeSet { .. } => (EVENT_MODE_SET, GBA_MODE_SET_BODY_BYTES),
        GbaSioMultiEvent::TransferStart { .. } => {
            (EVENT_TRANSFER_START, GBA_TRANSFER_START_BODY_BYTES)
        }
        GbaSioMultiEvent::TransferReply { .. } => {
            (EVENT_TRANSFER_REPLY, GBA_TRANSFER_REPLY_BODY_BYTES)
        }
        GbaSioMultiEvent::TransferCommit { .. } => {
            (EVENT_TRANSFER_COMMIT, GBA_TRANSFER_COMMIT_BODY_BYTES)
        }
        GbaSioMultiEvent::TransferAbort { .. } => {
            (EVENT_TRANSFER_ABORT, GBA_TRANSFER_ABORT_BODY_BYTES)
        }
        GbaSioMultiEvent::ModeAck { .. } => (EVENT_MODE_ACK, GBA_MODE_ACK_BODY_BYTES),
        GbaSioMultiEvent::FinishAck { .. } => (EVENT_FINISH_ACK, GBA_FINISH_ACK_BODY_BYTES),
    };
    debug_assert!(
        protocol == LinkCableWireProtocol::GbaSioMultiV2
            || !matches!(
                frame.event,
                GbaSioMultiEvent::ModeAck { .. } | GbaSioMultiEvent::FinishAck { .. }
            )
    );
    let mut encoded = encode_header(frame.header, event_kind, body_length)?;

    match &frame.event {
        GbaSioMultiEvent::ModeSet {
            mode,
            siocnt,
            rcnt,
            emulated_time,
        } => {
            encoded.push(*mode);
            push_u16(&mut encoded, *siocnt);
            push_u16(&mut encoded, *rcnt);
            push_u64(&mut encoded, *emulated_time);
        }
        GbaSioMultiEvent::TransferStart {
            transfer_id,
            siocnt,
            parent_word,
            emulated_time,
        } => {
            push_u32(&mut encoded, *transfer_id);
            encoded.push(GBA_SIO_MULTI_WIRE_MODE);
            push_u16(&mut encoded, *siocnt);
            push_u16(&mut encoded, *parent_word);
            push_u64(&mut encoded, *emulated_time);
        }
        GbaSioMultiEvent::TransferReply {
            transfer_id,
            child_word,
            emulated_time,
        } => {
            push_u32(&mut encoded, *transfer_id);
            push_u16(&mut encoded, *child_word);
            push_u64(&mut encoded, *emulated_time);
        }
        GbaSioMultiEvent::TransferCommit { transfer_id, words } => {
            push_u32(&mut encoded, *transfer_id);
            for word in words {
                push_u16(&mut encoded, *word);
            }
        }
        GbaSioMultiEvent::TransferAbort {
            transfer_id,
            reason,
        } => {
            push_u32(&mut encoded, *transfer_id);
            encoded.push(reason.wire_value());
        }
        GbaSioMultiEvent::ModeAck {
            acknowledged_mode_sender_sequence,
            emulated_time,
        } => {
            push_u64(&mut encoded, *acknowledged_mode_sender_sequence);
            push_u64(&mut encoded, *emulated_time);
        }
        GbaSioMultiEvent::FinishAck {
            transfer_id,
            emulated_time,
        } => {
            push_u32(&mut encoded, *transfer_id);
            push_u64(&mut encoded, *emulated_time);
        }
    }

    debug_assert_eq!(encoded.len(), LINK_CABLE_WIRE_HEADER_BYTES + body_length);
    Ok(encoded)
}

fn encode_valid_gb_frame(frame: &GbSerialFrame) -> Result<Vec<u8>, LinkCableWireCodecError> {
    let (event_kind, body_length) = match &frame.event {
        GbSerialEvent::Start { .. } => (EVENT_TRANSFER_START, GB_SERIAL_START_BODY_BYTES),
        GbSerialEvent::Reply { .. } => (EVENT_TRANSFER_REPLY, GB_SERIAL_REPLY_BODY_BYTES),
        GbSerialEvent::Commit { .. } => (EVENT_TRANSFER_COMMIT, GB_SERIAL_COMMIT_BODY_BYTES),
        GbSerialEvent::Abort { .. } => (EVENT_TRANSFER_ABORT, GB_SERIAL_ABORT_BODY_BYTES),
    };
    let mut encoded = encode_header(frame.header, event_kind, body_length)?;

    match &frame.event {
        GbSerialEvent::Start {
            transfer_id,
            clock_owner_slot,
            sc_control,
            owner_byte,
            emulated_time,
        } => {
            push_u32(&mut encoded, *transfer_id);
            encoded.push(*clock_owner_slot);
            encoded.push(*sc_control);
            encoded.push(*owner_byte);
            push_u64(&mut encoded, *emulated_time);
        }
        GbSerialEvent::Reply {
            transfer_id,
            clock_owner_slot,
            responder_byte,
            emulated_time,
        } => {
            push_u32(&mut encoded, *transfer_id);
            encoded.push(*clock_owner_slot);
            encoded.push(*responder_byte);
            push_u64(&mut encoded, *emulated_time);
        }
        GbSerialEvent::Commit {
            transfer_id,
            clock_owner_slot,
            slot_bytes,
        } => {
            push_u32(&mut encoded, *transfer_id);
            encoded.push(*clock_owner_slot);
            encoded.extend_from_slice(slot_bytes);
        }
        GbSerialEvent::Abort {
            transfer_id,
            clock_owner_slot,
            reason,
        } => {
            push_u32(&mut encoded, *transfer_id);
            encoded.push(*clock_owner_slot);
            encoded.push(reason.wire_value());
        }
    }

    debug_assert_eq!(encoded.len(), LINK_CABLE_WIRE_HEADER_BYTES + body_length);
    Ok(encoded)
}

fn encode_header(
    header: LinkCableWireHeader,
    event_kind: u8,
    body_length: usize,
) -> Result<Vec<u8>, LinkCableWireCodecError> {
    let frame_length = LINK_CABLE_WIRE_HEADER_BYTES + body_length;
    if frame_length > MAX_LINK_CABLE_WIRE_BYTES {
        return Err(LinkCableWireCodecError::InvalidFrameSize);
    }
    let body_length =
        u16::try_from(body_length).map_err(|_| LinkCableWireCodecError::InvalidFrameSize)?;
    let mut encoded = Vec::with_capacity(frame_length);
    encoded.extend_from_slice(&LINK_CABLE_MAGIC);
    encoded.push(LINK_CABLE_WIRE_VERSION);
    encoded.push(event_kind);
    push_u16(&mut encoded, RESERVED_FLAGS);
    push_u64(&mut encoded, header.room_epoch);
    push_u64(&mut encoded, header.session_epoch);
    push_u64(&mut encoded, header.cable_epoch);
    push_u64(&mut encoded, header.sender_sequence);
    encoded.push(header.sender_slot);
    push_u16(&mut encoded, body_length);
    Ok(encoded)
}

fn decode_header(
    payload: &[u8],
) -> Result<(LinkCableWireHeader, u8, &[u8]), LinkCableWireCodecError> {
    if !(LINK_CABLE_WIRE_HEADER_BYTES..=MAX_LINK_CABLE_WIRE_BYTES).contains(&payload.len()) {
        return Err(LinkCableWireCodecError::InvalidFrameSize);
    }
    if payload[0..4] != LINK_CABLE_MAGIC {
        return Err(LinkCableWireCodecError::UnsupportedMagic);
    }
    if payload[4] != LINK_CABLE_WIRE_VERSION {
        return Err(LinkCableWireCodecError::UnsupportedVersion);
    }
    if read_u16(payload, 6) != RESERVED_FLAGS {
        return Err(LinkCableWireCodecError::ReservedFlagsSet);
    }
    let body_length = usize::from(read_u16(payload, 41));
    if payload.len() != LINK_CABLE_WIRE_HEADER_BYTES + body_length {
        return Err(LinkCableWireCodecError::BodyLengthMismatch);
    }

    let header = LinkCableWireHeader {
        room_epoch: read_u64(payload, 8)?,
        session_epoch: read_u64(payload, 16)?,
        cable_epoch: read_u64(payload, 24)?,
        sender_sequence: read_u64(payload, 32)?,
        sender_slot: payload[40],
    };
    if header.sender_slot > 1 {
        return Err(LinkCableWireCodecError::InvalidSlot);
    }
    Ok((header, payload[5], &payload[LINK_CABLE_WIRE_HEADER_BYTES..]))
}

fn require_body_length(body: &[u8], expected: usize) -> Result<(), LinkCableWireCodecError> {
    if body.len() != expected {
        return Err(LinkCableWireCodecError::InvalidEventBodyLength);
    }
    Ok(())
}

fn push_u16(encoded: &mut Vec<u8>, value: u16) {
    encoded.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(encoded: &mut Vec<u8>, value: u32) {
    encoded.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(encoded: &mut Vec<u8>, value: u64) {
    encoded.extend_from_slice(&value.to_le_bytes());
}

fn read_u16(payload: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        payload[offset..offset + 2]
            .try_into()
            .expect("event body length checked"),
    )
}

fn read_u32(payload: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        payload[offset..offset + 4]
            .try_into()
            .expect("event body length checked"),
    )
}

fn read_u64(payload: &[u8], offset: usize) -> Result<u64, LinkCableWireCodecError> {
    let value = u64::from_le_bytes(
        payload[offset..offset + 8]
            .try_into()
            .expect("event body length checked"),
    );
    if value > i64::MAX as u64 {
        return Err(LinkCableWireCodecError::HighBitSet);
    }
    Ok(value)
}
