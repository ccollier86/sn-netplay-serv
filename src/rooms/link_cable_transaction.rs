//! Stateful validation for one live virtual-cable generation.
//!
//! The SBLK codec proves that one frame is well formed. This module additionally
//! proves that the frame is possible in the selected protocol's transaction
//! sequence before the private data plane forwards it to the peer.

use crate::protocol::{
    GbSerialEvent, GbaSioMultiEvent, GbaSioMultiFrame, LinkCableWireFrame, LinkCableWireProtocol,
};

const GBA_MULTI_MODE: u8 = 2;

/// A stateful SBLK transaction rule was violated.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum LinkCableTransactionError {
    /// The decoded frame did not use the room's frozen protocol namespace.
    #[error("link cable transaction frame used the wrong protocol")]
    ProtocolMismatch,
    /// Both GBA endpoints had not published MULTI mode for this cable epoch.
    #[error("both GBA endpoints must publish MULTI mode before a transfer")]
    GbaMultiModeNotReady,
    /// GBA left MULTI mode while a transfer boundary was stalled.
    #[error("GBA left MULTI mode while a link transfer was pending")]
    GbaModeChangedDuringTransfer,
    /// A v2 mode acknowledgement referenced an unknown or future opposite-slot mode.
    #[error("GBA mode acknowledgement exceeded the latest opposite-slot mode publication")]
    GbaModeAcknowledgementMismatch,
    /// Another transfer was already pending.
    #[error("a link cable transfer is already pending")]
    TransferAlreadyPending,
    /// The event required a pending transaction, but none existed.
    #[error("link cable event has no pending transfer")]
    NoPendingTransfer,
    /// The event was not valid in the pending transaction phase.
    #[error("link cable event arrived in the wrong transaction phase")]
    UnexpectedTransactionPhase,
    /// The transfer key did not match the pending or exact-next key.
    #[error("link cable transfer identifier is stale, reused, or out of order")]
    TransferIdMismatch,
    /// The sender exhausted its transfer-id namespace for this cable epoch.
    #[error("link cable transfer identifier space is exhausted")]
    TransferIdExhausted,
    /// A commit did not reproduce the words or bytes accepted at start/reply.
    #[error("link cable commit payload does not match the pending transfer")]
    CommitPayloadMismatch,
}

/// Stateful validator selected once by the room's frozen link protocol.
pub(crate) struct LinkCableTransactionState {
    protocol: LinkCableWireProtocol,
    state: ProtocolTransactionState,
}

enum ProtocolTransactionState {
    Gba(GbaTransactionState),
    Gb(GbTransactionState),
}

struct GbaTransactionState {
    version: GbaTransactionVersion,
    modes: [Option<u8>; 2],
    latest_mode_sequences: [Option<u64>; 2],
    pending_mode_acknowledgements: [Option<u64>; 2],
    next_transfer_id: TransferIdSequence,
    pending: Option<GbaPendingTransfer>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GbaTransactionVersion {
    V1,
    V2,
}

enum GbaPendingTransfer {
    Started {
        transfer_id: u32,
        parent_word: u16,
    },
    Replied {
        transfer_id: u32,
        parent_word: u16,
        child_word: u16,
    },
    AwaitingFinishAck {
        transfer_id: u32,
    },
}

#[derive(Default)]
struct GbTransactionState {
    next_transfer_ids: [TransferIdSequence; 2],
    pending: Option<GbPendingTransfer>,
}

enum GbPendingTransfer {
    Started {
        transfer_id: u32,
        clock_owner_slot: u8,
        owner_byte: u8,
    },
    Replied {
        transfer_id: u32,
        clock_owner_slot: u8,
        owner_byte: u8,
        responder_byte: u8,
    },
}

/// Exact-next transfer id whose `None` state means `u32::MAX` was consumed.
#[derive(Clone, Copy)]
struct TransferIdSequence {
    next: Option<u32>,
}

impl Default for TransferIdSequence {
    fn default() -> Self {
        Self { next: Some(1) }
    }
}

impl TransferIdSequence {
    fn accept(&mut self, transfer_id: u32) -> Result<(), LinkCableTransactionError> {
        let Some(expected) = self.next else {
            return Err(LinkCableTransactionError::TransferIdExhausted);
        };
        if transfer_id != expected {
            return Err(LinkCableTransactionError::TransferIdMismatch);
        }

        self.next = transfer_id.checked_add(1);
        Ok(())
    }
}

impl LinkCableTransactionState {
    /// Creates an empty transaction state for one new cable generation.
    pub(crate) fn new(protocol: LinkCableWireProtocol) -> Self {
        let state = match protocol {
            LinkCableWireProtocol::GbaSioMultiV1 => {
                ProtocolTransactionState::Gba(GbaTransactionState::new(GbaTransactionVersion::V1))
            }
            LinkCableWireProtocol::GbaSioMultiV2 => {
                ProtocolTransactionState::Gba(GbaTransactionState::new(GbaTransactionVersion::V2))
            }
            LinkCableWireProtocol::GbSerialV1 => {
                ProtocolTransactionState::Gb(GbTransactionState::default())
            }
        };

        Self { protocol, state }
    }

    /// Clears all modes, pending transfers, and per-owner transfer-id floors.
    pub(crate) fn reset(&mut self, protocol: LinkCableWireProtocol) {
        *self = Self::new(protocol);
    }

    /// Validates and atomically advances one decoded event.
    pub(crate) fn validate_and_apply(
        &mut self,
        frame: &LinkCableWireFrame,
    ) -> Result<(), LinkCableTransactionError> {
        if frame.protocol() != self.protocol {
            return Err(LinkCableTransactionError::ProtocolMismatch);
        }

        match (&mut self.state, frame) {
            (ProtocolTransactionState::Gba(state), LinkCableWireFrame::GbaSioMulti(frame)) => {
                state.validate_and_apply(frame)
            }
            (ProtocolTransactionState::Gba(state), LinkCableWireFrame::GbaSioMultiV2(frame)) => {
                state.validate_and_apply(frame)
            }
            (ProtocolTransactionState::Gb(state), LinkCableWireFrame::GbSerial(frame)) => {
                state.validate_and_apply(frame.header.sender_slot, &frame.event)
            }
            _ => Err(LinkCableTransactionError::ProtocolMismatch),
        }
    }
}

impl GbaTransactionState {
    fn new(version: GbaTransactionVersion) -> Self {
        Self {
            version,
            modes: [None, None],
            latest_mode_sequences: [None, None],
            pending_mode_acknowledgements: [None, None],
            next_transfer_id: TransferIdSequence::default(),
            pending: None,
        }
    }

    fn validate_and_apply(
        &mut self,
        frame: &GbaSioMultiFrame,
    ) -> Result<(), LinkCableTransactionError> {
        let sender_slot = usize::from(frame.header.sender_slot);
        match &frame.event {
            GbaSioMultiEvent::ModeSet { mode, .. } => {
                // mGBA can republish SIOCNT/RCNT snapshots while one physical
                // transfer is pending. Those snapshots are idempotent as long
                // as the endpoint remains in MULTI mode. V2 additionally treats
                // a slot-1 mode exit crossed with an unacknowledged START as a
                // rejected proposal, not as a failed cable generation.
                let cancels_crossed_start = self.version == GbaTransactionVersion::V2
                    && sender_slot == 1
                    && *mode != GBA_MULTI_MODE
                    && matches!(self.pending, Some(GbaPendingTransfer::Started { .. }));
                if cancels_crossed_start {
                    self.pending = None;
                } else if self.pending.is_some() && *mode != GBA_MULTI_MODE {
                    return Err(LinkCableTransactionError::GbaModeChangedDuringTransfer);
                }

                self.modes[sender_slot] = Some(*mode);
                if self.version == GbaTransactionVersion::V2 {
                    self.latest_mode_sequences[sender_slot] = Some(frame.header.sender_sequence);
                    self.pending_mode_acknowledgements[sender_slot] =
                        Some(frame.header.sender_sequence);
                }
            }
            GbaSioMultiEvent::ModeAck {
                acknowledged_mode_sender_sequence,
                ..
            } => {
                if self.version != GbaTransactionVersion::V2 {
                    return Err(LinkCableTransactionError::UnexpectedTransactionPhase);
                }
                let acknowledged_slot = 1 - sender_slot;
                let Some(latest_sequence) = self.latest_mode_sequences[acknowledged_slot] else {
                    return Err(LinkCableTransactionError::GbaModeAcknowledgementMismatch);
                };
                if *acknowledged_mode_sender_sequence > latest_sequence {
                    return Err(LinkCableTransactionError::GbaModeAcknowledgementMismatch);
                }
                if *acknowledged_mode_sender_sequence == latest_sequence {
                    self.pending_mode_acknowledgements[acknowledged_slot] = None;
                }
            }
            GbaSioMultiEvent::TransferStart {
                transfer_id,
                parent_word,
                ..
            } => {
                if self.pending.is_some() {
                    return Err(LinkCableTransactionError::TransferAlreadyPending);
                }
                if self.version == GbaTransactionVersion::V2 {
                    self.next_transfer_id.accept(*transfer_id)?;
                    if self.modes != [Some(GBA_MULTI_MODE), Some(GBA_MULTI_MODE)]
                        || self
                            .pending_mode_acknowledgements
                            .iter()
                            .any(Option::is_some)
                    {
                        // A START crossed a mode transition or its apply ACK.
                        // Consume/forward the exact-next proposal but remain
                        // idle so both endpoint transfer-id floors converge.
                        return Ok(());
                    }
                } else if self.modes != [Some(GBA_MULTI_MODE), Some(GBA_MULTI_MODE)] {
                    return Err(LinkCableTransactionError::GbaMultiModeNotReady);
                } else {
                    self.next_transfer_id.accept(*transfer_id)?;
                }
                self.pending = Some(GbaPendingTransfer::Started {
                    transfer_id: *transfer_id,
                    parent_word: *parent_word,
                });
            }
            GbaSioMultiEvent::TransferReply {
                transfer_id,
                child_word,
                ..
            } => {
                let Some(GbaPendingTransfer::Started {
                    transfer_id: pending_id,
                    parent_word,
                }) = self.pending.as_ref()
                else {
                    return Err(match self.pending {
                        Some(_) => LinkCableTransactionError::UnexpectedTransactionPhase,
                        None => LinkCableTransactionError::NoPendingTransfer,
                    });
                };
                if transfer_id != pending_id {
                    return Err(LinkCableTransactionError::TransferIdMismatch);
                }

                self.pending = Some(GbaPendingTransfer::Replied {
                    transfer_id: *pending_id,
                    parent_word: *parent_word,
                    child_word: *child_word,
                });
            }
            GbaSioMultiEvent::TransferCommit { transfer_id, words } => {
                let Some(GbaPendingTransfer::Replied {
                    transfer_id: pending_id,
                    parent_word,
                    child_word,
                }) = self.pending.as_ref()
                else {
                    return Err(match self.pending {
                        Some(_) => LinkCableTransactionError::UnexpectedTransactionPhase,
                        None => LinkCableTransactionError::NoPendingTransfer,
                    });
                };
                if transfer_id != pending_id {
                    return Err(LinkCableTransactionError::TransferIdMismatch);
                }
                if words[0] != *parent_word || words[1] != *child_word {
                    return Err(LinkCableTransactionError::CommitPayloadMismatch);
                }

                self.pending = if self.version == GbaTransactionVersion::V2 {
                    Some(GbaPendingTransfer::AwaitingFinishAck {
                        transfer_id: *transfer_id,
                    })
                } else {
                    None
                };
            }
            GbaSioMultiEvent::FinishAck { transfer_id, .. } => {
                if self.version != GbaTransactionVersion::V2 {
                    return Err(LinkCableTransactionError::UnexpectedTransactionPhase);
                }
                let Some(GbaPendingTransfer::AwaitingFinishAck {
                    transfer_id: pending_id,
                }) = self.pending.as_ref()
                else {
                    return Err(match self.pending {
                        Some(_) => LinkCableTransactionError::UnexpectedTransactionPhase,
                        None => LinkCableTransactionError::NoPendingTransfer,
                    });
                };
                if transfer_id != pending_id {
                    return Err(LinkCableTransactionError::TransferIdMismatch);
                }
                self.pending = None;
            }
            GbaSioMultiEvent::TransferAbort { transfer_id, .. } => {
                let Some(pending_id) = self.pending_transfer_id() else {
                    return Err(LinkCableTransactionError::NoPendingTransfer);
                };
                if *transfer_id != pending_id {
                    return Err(LinkCableTransactionError::TransferIdMismatch);
                }

                // The frozen wire contract permits either endpoint to abort.
                self.pending = None;
            }
        }

        Ok(())
    }

    fn pending_transfer_id(&self) -> Option<u32> {
        match self.pending.as_ref()? {
            GbaPendingTransfer::Started { transfer_id, .. }
            | GbaPendingTransfer::Replied { transfer_id, .. }
            | GbaPendingTransfer::AwaitingFinishAck { transfer_id } => Some(*transfer_id),
        }
    }
}

impl GbTransactionState {
    fn validate_and_apply(
        &mut self,
        _sender_slot: u8,
        event: &GbSerialEvent,
    ) -> Result<(), LinkCableTransactionError> {
        match event {
            GbSerialEvent::Start {
                transfer_id,
                clock_owner_slot,
                owner_byte,
                ..
            } => {
                if self.pending.is_some() {
                    return Err(LinkCableTransactionError::TransferAlreadyPending);
                }
                self.next_transfer_ids[usize::from(*clock_owner_slot)].accept(*transfer_id)?;
                self.pending = Some(GbPendingTransfer::Started {
                    transfer_id: *transfer_id,
                    clock_owner_slot: *clock_owner_slot,
                    owner_byte: *owner_byte,
                });
            }
            GbSerialEvent::Reply {
                transfer_id,
                clock_owner_slot,
                responder_byte,
                ..
            } => {
                let Some(GbPendingTransfer::Started {
                    transfer_id: pending_id,
                    clock_owner_slot: pending_owner,
                    owner_byte,
                }) = self.pending.as_ref()
                else {
                    return Err(match self.pending {
                        Some(_) => LinkCableTransactionError::UnexpectedTransactionPhase,
                        None => LinkCableTransactionError::NoPendingTransfer,
                    });
                };
                if transfer_id != pending_id || clock_owner_slot != pending_owner {
                    return Err(LinkCableTransactionError::TransferIdMismatch);
                }

                self.pending = Some(GbPendingTransfer::Replied {
                    transfer_id: *pending_id,
                    clock_owner_slot: *pending_owner,
                    owner_byte: *owner_byte,
                    responder_byte: *responder_byte,
                });
            }
            GbSerialEvent::Commit {
                transfer_id,
                clock_owner_slot,
                slot_bytes,
            } => {
                let Some(GbPendingTransfer::Replied {
                    transfer_id: pending_id,
                    clock_owner_slot: pending_owner,
                    owner_byte,
                    responder_byte,
                }) = self.pending.as_ref()
                else {
                    return Err(match self.pending {
                        Some(_) => LinkCableTransactionError::UnexpectedTransactionPhase,
                        None => LinkCableTransactionError::NoPendingTransfer,
                    });
                };
                if transfer_id != pending_id || clock_owner_slot != pending_owner {
                    return Err(LinkCableTransactionError::TransferIdMismatch);
                }
                let owner = usize::from(*pending_owner);
                let responder = 1 - owner;
                if slot_bytes[owner] != *owner_byte || slot_bytes[responder] != *responder_byte {
                    return Err(LinkCableTransactionError::CommitPayloadMismatch);
                }

                self.pending = None;
            }
            GbSerialEvent::Abort {
                transfer_id,
                clock_owner_slot,
                ..
            } => {
                let Some((pending_owner, pending_id)) = self.pending_transfer_key() else {
                    return Err(LinkCableTransactionError::NoPendingTransfer);
                };
                if *transfer_id != pending_id || *clock_owner_slot != pending_owner {
                    return Err(LinkCableTransactionError::TransferIdMismatch);
                }

                // The frozen wire contract permits either endpoint to abort.
                self.pending = None;
            }
        }

        Ok(())
    }

    fn pending_transfer_key(&self) -> Option<(u8, u32)> {
        match self.pending.as_ref()? {
            GbPendingTransfer::Started {
                transfer_id,
                clock_owner_slot,
                ..
            }
            | GbPendingTransfer::Replied {
                transfer_id,
                clock_owner_slot,
                ..
            } => Some((*clock_owner_slot, *transfer_id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LinkCableTransactionError, LinkCableTransactionState, TransferIdSequence};
    use crate::protocol::{
        GB_SERIAL_NORMAL_CLOCK_CONTROL, GbSerialEvent, GbSerialFrame, GbaSioMultiEvent,
        GbaSioMultiFrame, LinkCableAbortReason, LinkCableWireFrame, LinkCableWireHeader,
        LinkCableWireProtocol,
    };

    #[test]
    fn gba_happy_path_requires_both_modes_and_matching_commit_words() {
        let mut state = LinkCableTransactionState::new(LinkCableWireProtocol::GbaSioMultiV1);

        accept(&mut state, gba(0, mode_set(2)));
        assert_eq!(
            state.validate_and_apply(&gba(0, start(1, 0x1111))),
            Err(LinkCableTransactionError::GbaMultiModeNotReady)
        );

        // A cable abort resets this validator in production. Use a fresh state
        // to prove the accepted transaction sequence.
        let mut state = LinkCableTransactionState::new(LinkCableWireProtocol::GbaSioMultiV1);
        accept(&mut state, gba(0, mode_set(2)));
        accept(&mut state, gba(1, mode_set(2)));
        accept(&mut state, gba(0, start(1, 0x1111)));
        accept(&mut state, gba(1, reply(1, 0x2222)));
        accept(
            &mut state,
            gba(0, commit(1, [0x1111, 0x2222, 0xffff, 0xffff])),
        );
        accept(&mut state, gba(0, start(2, 0x3333)));
        accept(
            &mut state,
            gba(
                1,
                GbaSioMultiEvent::TransferAbort {
                    transfer_id: 2,
                    reason: LinkCableAbortReason::Timeout,
                },
            ),
        );
    }

    #[test]
    fn gba_rejects_phase_id_and_commit_content_violations() {
        let mut state = ready_gba();
        assert_eq!(
            state.validate_and_apply(&gba(1, reply(1, 2))),
            Err(LinkCableTransactionError::NoPendingTransfer)
        );
        accept(&mut state, gba(0, start(1, 0x1111)));
        accept(
            &mut state,
            gba(
                0,
                GbaSioMultiEvent::ModeSet {
                    mode: 2,
                    siocnt: 0x20f7,
                    rcnt: 0x7fff,
                    emulated_time: 99,
                },
            ),
        );
        assert_eq!(
            state.validate_and_apply(&gba(0, mode_set(3))),
            Err(LinkCableTransactionError::GbaModeChangedDuringTransfer)
        );
        assert_eq!(
            state.validate_and_apply(&gba(1, reply(2, 0x2222))),
            Err(LinkCableTransactionError::TransferIdMismatch)
        );
        accept(&mut state, gba(1, reply(1, 0x2222)));
        assert_eq!(
            state.validate_and_apply(&gba(0, commit(1, [0x1111, 0x9999, 0xffff, 0xffff]),)),
            Err(LinkCableTransactionError::CommitPayloadMismatch)
        );
    }

    #[test]
    fn gba_start_ids_are_exact_next_and_never_reused_after_abort() {
        let mut state = ready_gba();
        assert_eq!(
            state.validate_and_apply(&gba(0, start(2, 0))),
            Err(LinkCableTransactionError::TransferIdMismatch)
        );
        accept(&mut state, gba(0, start(1, 0)));
        accept(
            &mut state,
            gba(
                0,
                GbaSioMultiEvent::TransferAbort {
                    transfer_id: 1,
                    reason: LinkCableAbortReason::CoreClosed,
                },
            ),
        );
        assert_eq!(
            state.validate_and_apply(&gba(0, start(1, 0))),
            Err(LinkCableTransactionError::TransferIdMismatch)
        );
        accept(&mut state, gba(0, start(2, 0)));
    }

    #[test]
    fn gba_v2_requires_mode_apply_and_native_finish_acknowledgements() {
        let mut state = LinkCableTransactionState::new(LinkCableWireProtocol::GbaSioMultiV2);

        accept(&mut state, gba_v2(0, 0, mode_set(2)));
        accept(&mut state, gba_v2(0, 1, mode_set(2)));
        accept(&mut state, gba_v2(1, 0, mode_ack(0)));
        assert_eq!(
            state.validate_and_apply(&gba_v2(1, 0, mode_ack(99))),
            Err(LinkCableTransactionError::GbaModeAcknowledgementMismatch)
        );
        accept(&mut state, gba_v2(1, 2, mode_set(2)));
        accept(&mut state, gba_v2(0, 2, mode_ack(2)));

        // Slot zero's stale ACK did not release its superseding MODE_SET, so
        // this exact-next START is forwarded as a canceled proposal.
        accept(&mut state, gba_v2(0, 3, start(1, 0x1111)));
        assert_eq!(
            state.validate_and_apply(&gba_v2(1, 3, reply(1, 0x2222))),
            Err(LinkCableTransactionError::NoPendingTransfer)
        );
        accept(&mut state, gba_v2(1, 3, mode_ack(1)));

        accept(&mut state, gba_v2(0, 4, start(2, 0x1111)));
        accept(&mut state, gba_v2(1, 4, reply(2, 0x2222)));
        accept(
            &mut state,
            gba_v2(0, 5, commit(2, [0x1111, 0x2222, 0xffff, 0xffff])),
        );
        assert_eq!(
            state.validate_and_apply(&gba_v2(0, 6, start(3, 0x3333))),
            Err(LinkCableTransactionError::TransferAlreadyPending)
        );
        assert_eq!(
            state.validate_and_apply(&gba_v2(1, 5, finish_ack(1))),
            Err(LinkCableTransactionError::TransferIdMismatch)
        );
        accept(&mut state, gba_v2(1, 5, finish_ack(2)));
        accept(&mut state, gba_v2(0, 6, start(3, 0x3333)));
    }

    #[test]
    fn gba_v2_nonfatally_cancels_both_start_mode_crossing_orders_before_reply() {
        let mut start_first = ready_gba_v2();
        accept(&mut start_first, gba_v2(0, 2, start(1, 0x1111)));
        accept(&mut start_first, gba_v2(1, 2, mode_set(0)));
        assert_eq!(
            start_first.validate_and_apply(&gba_v2(1, 3, reply(1, 0x2222))),
            Err(LinkCableTransactionError::NoPendingTransfer)
        );
        accept(&mut start_first, gba_v2(0, 3, mode_ack(2)));
        accept(&mut start_first, gba_v2(0, 4, start(2, 0x3333)));
        assert_eq!(
            start_first.validate_and_apply(&gba_v2(1, 4, reply(2, 0x4444))),
            Err(LinkCableTransactionError::NoPendingTransfer)
        );

        let mut mode_first = ready_gba_v2();
        accept(&mut mode_first, gba_v2(1, 2, mode_set(0)));
        accept(&mut mode_first, gba_v2(1, 3, mode_set(2)));
        accept(&mut mode_first, gba_v2(0, 2, mode_ack(2)));
        accept(&mut mode_first, gba_v2(0, 3, start(1, 0x1111)));
        accept(&mut mode_first, gba_v2(0, 4, mode_ack(3)));
        assert_eq!(
            mode_first.validate_and_apply(&gba_v2(0, 5, start(1, 0x2222))),
            Err(LinkCableTransactionError::TransferIdMismatch)
        );
        accept(&mut mode_first, gba_v2(0, 5, start(2, 0x2222)));
    }

    #[test]
    fn gba_v2_mode_exit_remains_fatal_after_reply_or_commit() {
        let mut after_reply = ready_gba_v2();
        accept(&mut after_reply, gba_v2(0, 2, start(1, 0x1111)));
        accept(&mut after_reply, gba_v2(1, 2, reply(1, 0x2222)));
        assert_eq!(
            after_reply.validate_and_apply(&gba_v2(1, 3, mode_set(0))),
            Err(LinkCableTransactionError::GbaModeChangedDuringTransfer)
        );

        let mut after_commit = ready_gba_v2();
        accept(&mut after_commit, gba_v2(0, 2, start(1, 0x1111)));
        accept(&mut after_commit, gba_v2(1, 2, reply(1, 0x2222)));
        accept(
            &mut after_commit,
            gba_v2(0, 3, commit(1, [0x1111, 0x2222, 0xffff, 0xffff])),
        );
        assert_eq!(
            after_commit.validate_and_apply(&gba_v2(1, 3, mode_set(0))),
            Err(LinkCableTransactionError::GbaModeChangedDuringTransfer)
        );
    }

    #[test]
    fn gb_happy_paths_keep_independent_owner_id_sequences() {
        let mut state = LinkCableTransactionState::new(LinkCableWireProtocol::GbSerialV1);

        accept(&mut state, gb(0, gb_start(1, 0, 0xa5)));
        accept(&mut state, gb(1, gb_reply(1, 0, 0x3c)));
        accept(&mut state, gb(0, gb_commit(1, 0, [0xa5, 0x3c])));

        accept(&mut state, gb(1, gb_start(1, 1, 0x5a)));
        accept(&mut state, gb(0, gb_reply(1, 1, 0xc3)));
        accept(&mut state, gb(1, gb_commit(1, 1, [0xc3, 0x5a])));

        accept(&mut state, gb(0, gb_start(2, 0, 0x10)));
        accept(
            &mut state,
            gb(
                1,
                GbSerialEvent::Abort {
                    transfer_id: 2,
                    clock_owner_slot: 0,
                    reason: LinkCableAbortReason::Timeout,
                },
            ),
        );
    }

    #[test]
    fn gb_rejects_collisions_wrong_phases_ids_and_commit_bytes() {
        let mut state = LinkCableTransactionState::new(LinkCableWireProtocol::GbSerialV1);
        assert_eq!(
            state.validate_and_apply(&gb(1, gb_reply(1, 0, 0))),
            Err(LinkCableTransactionError::NoPendingTransfer)
        );
        accept(&mut state, gb(0, gb_start(1, 0, 0xa5)));
        assert_eq!(
            state.validate_and_apply(&gb(1, gb_start(1, 1, 0x5a))),
            Err(LinkCableTransactionError::TransferAlreadyPending)
        );
        assert_eq!(
            state.validate_and_apply(&gb(1, gb_reply(2, 0, 0x3c))),
            Err(LinkCableTransactionError::TransferIdMismatch)
        );
        accept(&mut state, gb(1, gb_reply(1, 0, 0x3c)));
        assert_eq!(
            state.validate_and_apply(&gb(0, gb_commit(1, 0, [0xa5, 0xff]))),
            Err(LinkCableTransactionError::CommitPayloadMismatch)
        );
    }

    #[test]
    fn reset_clears_modes_pending_work_and_transfer_id_floors() {
        let mut state = ready_gba();
        accept(&mut state, gba(0, start(1, 0)));
        state.reset(LinkCableWireProtocol::GbaSioMultiV1);

        assert_eq!(
            state.validate_and_apply(&gba(0, start(1, 0))),
            Err(LinkCableTransactionError::GbaMultiModeNotReady)
        );
        accept(&mut state, gba(0, mode_set(2)));
        accept(&mut state, gba(1, mode_set(2)));
        accept(&mut state, gba(0, start(1, 0)));
    }

    #[test]
    fn transfer_id_sequence_never_wraps_after_u32_max() {
        let mut sequence = TransferIdSequence {
            next: Some(u32::MAX),
        };

        sequence.accept(u32::MAX).expect("final transfer id");
        assert_eq!(
            sequence.accept(1),
            Err(LinkCableTransactionError::TransferIdExhausted)
        );
    }

    fn ready_gba() -> LinkCableTransactionState {
        let mut state = LinkCableTransactionState::new(LinkCableWireProtocol::GbaSioMultiV1);
        accept(&mut state, gba(0, mode_set(2)));
        accept(&mut state, gba(1, mode_set(2)));
        state
    }

    fn ready_gba_v2() -> LinkCableTransactionState {
        let mut state = LinkCableTransactionState::new(LinkCableWireProtocol::GbaSioMultiV2);
        accept(&mut state, gba_v2(0, 0, mode_set(2)));
        accept(&mut state, gba_v2(1, 0, mode_ack(0)));
        accept(&mut state, gba_v2(1, 1, mode_set(2)));
        accept(&mut state, gba_v2(0, 1, mode_ack(1)));
        state
    }

    fn accept(state: &mut LinkCableTransactionState, frame: LinkCableWireFrame) {
        state
            .validate_and_apply(&frame)
            .expect("valid transaction event");
    }

    fn header(sender_slot: u8) -> LinkCableWireHeader {
        LinkCableWireHeader {
            room_epoch: 1,
            session_epoch: 1,
            cable_epoch: 1,
            sender_sequence: 0,
            sender_slot,
        }
    }

    fn gba(sender_slot: u8, event: GbaSioMultiEvent) -> LinkCableWireFrame {
        GbaSioMultiFrame {
            header: header(sender_slot),
            event,
        }
        .into()
    }

    fn gba_v2(
        sender_slot: u8,
        sender_sequence: u64,
        event: GbaSioMultiEvent,
    ) -> LinkCableWireFrame {
        let mut header = header(sender_slot);
        header.sender_sequence = sender_sequence;
        LinkCableWireFrame::GbaSioMultiV2(GbaSioMultiFrame { header, event })
    }

    fn gb(sender_slot: u8, event: GbSerialEvent) -> LinkCableWireFrame {
        GbSerialFrame {
            header: header(sender_slot),
            event,
        }
        .into()
    }

    fn mode_set(mode: u8) -> GbaSioMultiEvent {
        GbaSioMultiEvent::ModeSet {
            mode,
            siocnt: 0,
            rcnt: 0,
            emulated_time: 0,
        }
    }

    fn start(transfer_id: u32, parent_word: u16) -> GbaSioMultiEvent {
        GbaSioMultiEvent::TransferStart {
            transfer_id,
            siocnt: 0x2000,
            parent_word,
            emulated_time: 0,
        }
    }

    fn reply(transfer_id: u32, child_word: u16) -> GbaSioMultiEvent {
        GbaSioMultiEvent::TransferReply {
            transfer_id,
            child_word,
            emulated_time: 0,
        }
    }

    fn commit(transfer_id: u32, words: [u16; 4]) -> GbaSioMultiEvent {
        GbaSioMultiEvent::TransferCommit { transfer_id, words }
    }

    fn mode_ack(acknowledged_mode_sender_sequence: u64) -> GbaSioMultiEvent {
        GbaSioMultiEvent::ModeAck {
            acknowledged_mode_sender_sequence,
            emulated_time: 0,
        }
    }

    fn finish_ack(transfer_id: u32) -> GbaSioMultiEvent {
        GbaSioMultiEvent::FinishAck {
            transfer_id,
            emulated_time: 0,
        }
    }

    fn gb_start(transfer_id: u32, owner: u8, owner_byte: u8) -> GbSerialEvent {
        GbSerialEvent::Start {
            transfer_id,
            clock_owner_slot: owner,
            sc_control: GB_SERIAL_NORMAL_CLOCK_CONTROL,
            owner_byte,
            emulated_time: 0,
        }
    }

    fn gb_reply(transfer_id: u32, owner: u8, responder_byte: u8) -> GbSerialEvent {
        GbSerialEvent::Reply {
            transfer_id,
            clock_owner_slot: owner,
            responder_byte,
            emulated_time: 0,
        }
    }

    fn gb_commit(transfer_id: u32, owner: u8, slot_bytes: [u8; 2]) -> GbSerialEvent {
        GbSerialEvent::Commit {
            transfer_id,
            clock_owner_slot: owner,
            slot_bytes,
        }
    }
}
