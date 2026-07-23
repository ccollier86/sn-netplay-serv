//! Frozen `SBLK` v1 emulator-event frame codec.
//!
//! The 43-byte common header intentionally omits a family discriminator. The
//! authoritative link-session descriptor selects `gba-sio-multi-v1` or
//! `gb-serial-v1` before decoding. This module validates byte shape and
//! event-local invariants only; a room provider must additionally validate the
//! authenticated route and slot, authoritative epochs, exact-next sequence,
//! and transaction state before enqueueing a decoded event.

mod codec;
mod error;
mod model;
mod validation;

pub use codec::{
    decode_gb_serial_frame, decode_gba_sio_multi_frame, decode_link_cable_wire_frame,
    encode_gb_serial_frame, encode_gba_sio_multi_frame, encode_link_cable_wire_frame,
};
pub use error::LinkCableWireCodecError;
pub use model::{
    GbSerialEvent, GbSerialFrame, GbaSioMultiEvent, GbaSioMultiFrame, LinkCableAbortReason,
    LinkCableWireFrame, LinkCableWireHeader, LinkCableWireProtocol,
};

/// Frozen SBLK wire version.
pub const LINK_CABLE_WIRE_VERSION: u8 = 1;
/// Fixed SBLK v1 header bytes before the event body.
pub const LINK_CABLE_WIRE_HEADER_BYTES: usize = 43;
/// Maximum complete SBLK v1 frame bytes.
pub const MAX_LINK_CABLE_WIRE_BYTES: usize = 128;
/// GBA SIO multiplayer mode encoded by a transfer-start body.
pub const GBA_SIO_MULTI_WIRE_MODE: u8 = 2;
/// Disconnected GBA multiplayer word required for absent slots.
pub const GBA_MULTI_DISCONNECTED_WORD: u16 = 0xffff;
/// GB serial internal normal-clock control value.
pub const GB_SERIAL_NORMAL_CLOCK_CONTROL: u8 = 0x81;
/// GB serial internal fast-clock control value.
pub const GB_SERIAL_FAST_CLOCK_CONTROL: u8 = 0x83;

#[cfg(test)]
mod tests;
