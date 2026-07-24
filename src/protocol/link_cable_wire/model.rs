//! Protocol-explicit SBLK v1 frame models.

use super::LinkCableWireCodecError;

/// Body namespace selected by the authoritative link session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkCableWireProtocol {
    /// Two-player GBA SIO multiplayer events.
    GbaSioMultiV1,
    /// Two-player GBA SIO multiplayer events with apply/finish barriers.
    GbaSioMultiV2,
    /// Two-player GB/GBC serial events.
    GbSerialV1,
}

impl LinkCableWireProtocol {
    /// Exact descriptor value that selects this body namespace.
    pub const fn wire_value(self) -> &'static str {
        match self {
            Self::GbaSioMultiV1 => "gba-sio-multi-v1",
            Self::GbaSioMultiV2 => "gba-sio-multi-v2",
            Self::GbSerialV1 => "gb-serial-v1",
        }
    }
}

impl TryFrom<&str> for LinkCableWireProtocol {
    type Error = LinkCableWireCodecError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "gba-sio-multi-v1" => Ok(Self::GbaSioMultiV1),
            "gba-sio-multi-v2" => Ok(Self::GbaSioMultiV2),
            "gb-serial-v1" => Ok(Self::GbSerialV1),
            _ => Err(LinkCableWireCodecError::UnsupportedProtocol),
        }
    }
}

/// Shared SBLK v1 routing and ordering header fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkCableWireHeader {
    /// Authoritative room-incarnation generation.
    pub room_epoch: u64,
    /// Authoritative gameplay-provider generation.
    pub session_epoch: u64,
    /// Authoritative live cable-attachment generation.
    pub cable_epoch: u64,
    /// Sender-local exact-next sequence within the cable epoch.
    pub sender_sequence: u64,
    /// Authenticated lobby slot, always 0 or 1.
    pub sender_slot: u8,
}

/// SBLK v1 frame decoded with its authoritative protocol namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkCableWireFrame {
    /// Frozen GBA SIO multiplayer v1 namespace.
    GbaSioMulti(GbaSioMultiFrame),
    /// GBA SIO multiplayer v2 namespace with causal barriers.
    GbaSioMultiV2(GbaSioMultiFrame),
    /// GB/GBC serial namespace.
    GbSerial(GbSerialFrame),
}

impl LinkCableWireFrame {
    /// Protocol namespace encoded by this typed frame.
    pub const fn protocol(&self) -> LinkCableWireProtocol {
        match self {
            Self::GbaSioMulti(_) => LinkCableWireProtocol::GbaSioMultiV1,
            Self::GbaSioMultiV2(_) => LinkCableWireProtocol::GbaSioMultiV2,
            Self::GbSerial(_) => LinkCableWireProtocol::GbSerialV1,
        }
    }

    /// Shared SBLK v1 header.
    pub const fn header(&self) -> &LinkCableWireHeader {
        match self {
            Self::GbaSioMulti(frame) | Self::GbaSioMultiV2(frame) => &frame.header,
            Self::GbSerial(frame) => &frame.header,
        }
    }
}

impl From<GbaSioMultiFrame> for LinkCableWireFrame {
    fn from(frame: GbaSioMultiFrame) -> Self {
        Self::GbaSioMulti(frame)
    }
}

impl From<GbSerialFrame> for LinkCableWireFrame {
    fn from(frame: GbSerialFrame) -> Self {
        Self::GbSerial(frame)
    }
}

/// One typed `gba-sio-multi-v1` or `gba-sio-multi-v2` SBLK frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GbaSioMultiFrame {
    /// Shared SBLK v1 header.
    pub header: LinkCableWireHeader,
    /// GBA SIO multiplayer event body.
    pub event: GbaSioMultiEvent,
}

/// Event bodies shared by the GBA SIO namespaces. Barrier events are v2-only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GbaSioMultiEvent {
    /// Once-per-epoch baseline or a subsequent GBA SIO mode transition.
    ModeSet {
        /// Explicit mGBA SIO mode: 0, 1, 2, 3, 8, or 12.
        mode: u8,
        /// Raw SIOCNT register snapshot.
        siocnt: u16,
        /// Raw RCNT register snapshot.
        rcnt: u16,
        /// Sender-local non-negative emulated cycle timestamp.
        emulated_time: u64,
    },
    /// Slot 0 starts a two-player SIO multiplayer transfer.
    TransferStart {
        /// Nonzero slot-0 transfer identifier.
        transfer_id: u32,
        /// Raw pre-start SIOCNT register snapshot.
        siocnt: u16,
        /// Slot-0 submitted multiplayer word.
        parent_word: u16,
        /// Sender-local non-negative emulated cycle timestamp.
        emulated_time: u64,
    },
    /// Slot 1 supplies its word for the pending transfer.
    TransferReply {
        /// Nonzero transfer identifier from the matching start.
        transfer_id: u32,
        /// Slot-1 submitted multiplayer word.
        child_word: u16,
        /// Sender-local non-negative emulated cycle timestamp.
        emulated_time: u64,
    },
    /// Slot 0 commits the complete two-player transfer result.
    TransferCommit {
        /// Nonzero transfer identifier from the matching start.
        transfer_id: u32,
        /// Slot words in lobby-slot order; entries 2 and 3 must be `0xffff`.
        words: [u16; 4],
    },
    /// Either slot aborts the pending transfer.
    TransferAbort {
        /// Nonzero transfer identifier.
        transfer_id: u32,
        /// Frozen cross-family abort reason.
        reason: LinkCableAbortReason,
    },
    /// The opposite endpoint applied one v2 mode snapshot.
    ModeAck {
        /// Sender sequence of the exact opposite-slot `MODE_SET`.
        acknowledged_mode_sender_sequence: u64,
        /// Acknowledging endpoint's non-negative emulated cycle timestamp.
        emulated_time: u64,
    },
    /// Slot 1 completed the native multiplayer event for one v2 commit.
    FinishAck {
        /// Nonzero transfer identifier from the applied commit.
        transfer_id: u32,
        /// Responder's non-negative native-completion cycle timestamp.
        emulated_time: u64,
    },
}

/// One typed `gb-serial-v1` SBLK frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GbSerialFrame {
    /// Shared SBLK v1 header.
    pub header: LinkCableWireHeader,
    /// GB/GBC serial event body.
    pub event: GbSerialEvent,
}

/// Event bodies in the `gb-serial-v1` namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GbSerialEvent {
    /// The dynamic clock owner starts one serial transfer.
    Start {
        /// Nonzero identifier allocated by the clock-owning slot.
        transfer_id: u32,
        /// Dynamic clock owner, always 0 or 1.
        clock_owner_slot: u8,
        /// Exact internal-clock control value, `0x81` or `0x83`.
        sc_control: u8,
        /// Clock owner's submitted SB byte.
        owner_byte: u8,
        /// Sender-local non-negative emulated cycle timestamp.
        emulated_time: u64,
    },
    /// The non-owner supplies its byte for the pending transfer.
    Reply {
        /// Nonzero identifier from the matching start.
        transfer_id: u32,
        /// Dynamic clock owner, always 0 or 1.
        clock_owner_slot: u8,
        /// Responding slot's submitted SB byte.
        responder_byte: u8,
        /// Sender-local non-negative emulated cycle timestamp.
        emulated_time: u64,
    },
    /// The clock owner commits both submitted bytes.
    Commit {
        /// Nonzero identifier from the matching start.
        transfer_id: u32,
        /// Dynamic clock owner, always 0 or 1.
        clock_owner_slot: u8,
        /// Submitted bytes in lobby-slot order.
        slot_bytes: [u8; 2],
    },
    /// Either slot aborts the pending transfer.
    Abort {
        /// Nonzero transfer identifier.
        transfer_id: u32,
        /// Dynamic clock owner, always 0 or 1.
        clock_owner_slot: u8,
        /// Frozen cross-family abort reason.
        reason: LinkCableAbortReason,
    },
}

/// Frozen SBLK v1 transfer-abort reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum LinkCableAbortReason {
    /// The required response exceeded the negotiated stall timeout.
    Timeout = 1,
    /// A byte-shape, role, sequence, epoch, or transaction rule was violated.
    ProtocolViolation = 2,
    /// A required bounded queue could not accept the event.
    QueueOverflow = 3,
    /// The peer runtime detached.
    PeerDisconnected = 4,
    /// The local emulator core closed.
    CoreClosed = 5,
}

impl LinkCableAbortReason {
    pub(super) const fn wire_value(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for LinkCableAbortReason {
    type Error = LinkCableWireCodecError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Timeout),
            2 => Ok(Self::ProtocolViolation),
            3 => Ok(Self::QueueOverflow),
            4 => Ok(Self::PeerDisconnected),
            5 => Ok(Self::CoreClosed),
            _ => Err(LinkCableWireCodecError::InvalidAbortReason),
        }
    }
}
