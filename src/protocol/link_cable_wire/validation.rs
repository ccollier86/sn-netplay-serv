//! Event-local SBLK v1 validation shared by encoders and decoders.

use super::{
    GB_SERIAL_FAST_CLOCK_CONTROL, GB_SERIAL_NORMAL_CLOCK_CONTROL, GBA_MULTI_DISCONNECTED_WORD,
    GbSerialEvent, GbSerialFrame, GbaSioMultiEvent, GbaSioMultiFrame, LinkCableWireCodecError,
    LinkCableWireFrame, LinkCableWireHeader, LinkCableWireProtocol,
};

const MAX_SIGNED_U64: u64 = i64::MAX as u64;
const GBA_SIO_MODE_BITS: u16 = 0x3000;
const GBA_RCNT_MODE_BITS: u16 = 0xc000;
const GBA_SIO_MULTI_MODE_BITS: u16 = 0x2000;
const GBA_SIO_BUSY_BIT: u16 = 0x0080;

pub(super) fn validate_wire_frame(
    frame: &LinkCableWireFrame,
) -> Result<(), LinkCableWireCodecError> {
    match frame {
        LinkCableWireFrame::GbaSioMulti(frame) => {
            validate_gba_frame(LinkCableWireProtocol::GbaSioMultiV1, frame)
        }
        LinkCableWireFrame::GbaSioMultiV2(frame) => {
            validate_gba_frame(LinkCableWireProtocol::GbaSioMultiV2, frame)
        }
        LinkCableWireFrame::GbSerial(frame) => validate_gb_frame(frame),
    }
}

pub(super) fn validate_gba_frame(
    protocol: LinkCableWireProtocol,
    frame: &GbaSioMultiFrame,
) -> Result<(), LinkCableWireCodecError> {
    validate_header(&frame.header)?;

    match &frame.event {
        GbaSioMultiEvent::ModeSet {
            mode,
            siocnt,
            rcnt,
            emulated_time,
        } => {
            validate_non_negative_u64(*emulated_time)?;
            if !matches!(*mode, 0 | 1 | 2 | 3 | 8 | 12)
                || derive_gba_sio_mode(*siocnt, *rcnt) != *mode
            {
                return Err(LinkCableWireCodecError::InvalidGbaMode);
            }
        }
        GbaSioMultiEvent::TransferStart {
            transfer_id,
            siocnt,
            emulated_time,
            ..
        } => {
            validate_transfer_id(*transfer_id)?;
            validate_non_negative_u64(*emulated_time)?;
            if frame.header.sender_slot != 0 {
                return Err(LinkCableWireCodecError::InvalidEventRole);
            }
            if siocnt & GBA_SIO_MODE_BITS != GBA_SIO_MULTI_MODE_BITS
                || siocnt & GBA_SIO_BUSY_BIT != 0
            {
                return Err(LinkCableWireCodecError::InvalidGbaTransferStart);
            }
        }
        GbaSioMultiEvent::TransferReply {
            transfer_id,
            emulated_time,
            ..
        } => {
            validate_transfer_id(*transfer_id)?;
            validate_non_negative_u64(*emulated_time)?;
            if frame.header.sender_slot != 1 {
                return Err(LinkCableWireCodecError::InvalidEventRole);
            }
        }
        GbaSioMultiEvent::TransferCommit { transfer_id, words } => {
            validate_transfer_id(*transfer_id)?;
            if frame.header.sender_slot != 0 {
                return Err(LinkCableWireCodecError::InvalidEventRole);
            }
            if words[2] != GBA_MULTI_DISCONNECTED_WORD || words[3] != GBA_MULTI_DISCONNECTED_WORD {
                return Err(LinkCableWireCodecError::InvalidGbaDisconnectedWords);
            }
        }
        GbaSioMultiEvent::TransferAbort { transfer_id, .. } => {
            validate_transfer_id(*transfer_id)?;
        }
        GbaSioMultiEvent::ModeAck {
            acknowledged_mode_sender_sequence,
            emulated_time,
        } => {
            if protocol != LinkCableWireProtocol::GbaSioMultiV2 {
                return Err(LinkCableWireCodecError::UnsupportedEventKind);
            }
            validate_non_negative_u64(*acknowledged_mode_sender_sequence)?;
            validate_non_negative_u64(*emulated_time)?;
        }
        GbaSioMultiEvent::FinishAck {
            transfer_id,
            emulated_time,
        } => {
            if protocol != LinkCableWireProtocol::GbaSioMultiV2 {
                return Err(LinkCableWireCodecError::UnsupportedEventKind);
            }
            validate_transfer_id(*transfer_id)?;
            validate_non_negative_u64(*emulated_time)?;
            if frame.header.sender_slot != 1 {
                return Err(LinkCableWireCodecError::InvalidEventRole);
            }
        }
    }

    Ok(())
}

pub(super) fn validate_gb_frame(frame: &GbSerialFrame) -> Result<(), LinkCableWireCodecError> {
    validate_header(&frame.header)?;

    match &frame.event {
        GbSerialEvent::Start {
            transfer_id,
            clock_owner_slot,
            sc_control,
            emulated_time,
            ..
        } => {
            validate_transfer_id(*transfer_id)?;
            validate_slot(*clock_owner_slot)?;
            validate_non_negative_u64(*emulated_time)?;
            if *clock_owner_slot != frame.header.sender_slot {
                return Err(LinkCableWireCodecError::InvalidEventRole);
            }
            if !matches!(
                *sc_control,
                GB_SERIAL_NORMAL_CLOCK_CONTROL | GB_SERIAL_FAST_CLOCK_CONTROL
            ) {
                return Err(LinkCableWireCodecError::InvalidGbSerialControl);
            }
        }
        GbSerialEvent::Reply {
            transfer_id,
            clock_owner_slot,
            emulated_time,
            ..
        } => {
            validate_transfer_id(*transfer_id)?;
            validate_slot(*clock_owner_slot)?;
            validate_non_negative_u64(*emulated_time)?;
            if frame.header.sender_slot != 1 - *clock_owner_slot {
                return Err(LinkCableWireCodecError::InvalidEventRole);
            }
        }
        GbSerialEvent::Commit {
            transfer_id,
            clock_owner_slot,
            ..
        } => {
            validate_transfer_id(*transfer_id)?;
            validate_slot(*clock_owner_slot)?;
            if frame.header.sender_slot != *clock_owner_slot {
                return Err(LinkCableWireCodecError::InvalidEventRole);
            }
        }
        GbSerialEvent::Abort {
            transfer_id,
            clock_owner_slot,
            ..
        } => {
            validate_transfer_id(*transfer_id)?;
            validate_slot(*clock_owner_slot)?;
        }
    }

    Ok(())
}

fn validate_header(header: &LinkCableWireHeader) -> Result<(), LinkCableWireCodecError> {
    validate_non_negative_u64(header.room_epoch)?;
    validate_non_negative_u64(header.session_epoch)?;
    validate_non_negative_u64(header.cable_epoch)?;
    validate_non_negative_u64(header.sender_sequence)?;
    validate_slot(header.sender_slot)
}

fn validate_non_negative_u64(value: u64) -> Result<(), LinkCableWireCodecError> {
    if value > MAX_SIGNED_U64 {
        return Err(LinkCableWireCodecError::HighBitSet);
    }
    Ok(())
}

fn validate_slot(slot: u8) -> Result<(), LinkCableWireCodecError> {
    if slot > 1 {
        return Err(LinkCableWireCodecError::InvalidSlot);
    }
    Ok(())
}

fn validate_transfer_id(transfer_id: u32) -> Result<(), LinkCableWireCodecError> {
    if transfer_id == 0 {
        return Err(LinkCableWireCodecError::InvalidTransferId);
    }
    Ok(())
}

fn derive_gba_sio_mode(siocnt: u16, rcnt: u16) -> u8 {
    let encoded_mode = ((rcnt & GBA_RCNT_MODE_BITS) | (siocnt & GBA_SIO_MODE_BITS)) >> 12;
    if encoded_mode < 8 {
        (encoded_mode & 0x3) as u8
    } else {
        (encoded_mode & 0xc) as u8
    }
}
