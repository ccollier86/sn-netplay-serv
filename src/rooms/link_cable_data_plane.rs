//! Private, bounded data plane for one mGBA link-cable room.
//!
//! This module deliberately owns no controller-room events, debug history, or
//! registry lock. A registry may clone the handle while holding its own lock,
//! release that lock, and then perform all link validation and forwarding
//! against this per-room state.

use crate::protocol::{
    GbSerialEvent, GbaSioMultiEvent, LINK_CABLE_WIRE_HEADER_BYTES, LinkCableAbortReason,
    LinkCablePacket, LinkCableWireCodecError, LinkCableWireFrame, LinkCableWireProtocol,
    MAX_LINK_CABLE_WIRE_BYTES, decode_link_cable_wire_frame,
};
use crate::rooms::{
    ConnectionId, LinkCableTransactionError, LinkCableTransactionState, PlayerIndex, RoomScope,
};
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use tokio::sync::Notify;

const LINK_PLAYER_COUNT: usize = 2;
const MAX_SIGNED_U64: u64 = i64::MAX as u64;

/// Lifecycle state visible only through an authenticated link-private route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkCableDataPlaneStatus {
    /// Fewer than two endpoint receivers are attached.
    Waiting,
    /// Both endpoints are attached under the current live cable generation.
    Active,
    /// The prior cable generation was invalidated and must be reattached.
    Aborted,
    /// The provider was permanently closed.
    Closed,
}

/// Private lifecycle projection for one endpoint.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct LinkCableDataPlaneSnapshot {
    /// Stable server-issued scope for the authoritative room.
    pub(crate) room_scope: u64,
    /// Current authoritative room generation.
    pub(crate) room_epoch: u64,
    /// Current authoritative gameplay-provider generation.
    pub(crate) session_epoch: u64,
    /// Current or most recently allocated cable generation.
    pub(crate) cable_epoch: u64,
    /// Maximum required events retained for either target endpoint.
    pub(crate) queue_capacity: usize,
    /// Frozen event namespace selected by the room descriptor.
    pub(crate) protocol: LinkCableWireProtocol,
    /// Current provider lifecycle state.
    pub(crate) status: LinkCableDataPlaneStatus,
    /// Cause of the most recent cable abort, when one maps to SBLK v1.
    pub(crate) abort_reason: Option<LinkCableAbortReason>,
    /// Monotonic local lifecycle revision used to order state before packets.
    pub(crate) lifecycle_revision: u64,
    /// Authenticated endpoint receiving this private projection.
    pub(crate) local_slot: PlayerIndex,
}

impl fmt::Debug for LinkCableDataPlaneSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinkCableDataPlaneSnapshot")
            .field("room_scope", &"<redacted>")
            .field("room_epoch", &self.room_epoch)
            .field("session_epoch", &self.session_epoch)
            .field("cable_epoch", &self.cable_epoch)
            .field("protocol", &self.protocol)
            .field("status", &self.status)
            .field("abort_reason", &self.abort_reason)
            .field("lifecycle_revision", &self.lifecycle_revision)
            .field("queue_capacity", &self.queue_capacity)
            .field("local_slot", &self.local_slot)
            .finish()
    }
}

/// One item delivered to exactly one attached endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkCableDataPlaneEvent {
    /// A fully validated packet emitted by the other endpoint.
    Packet(LinkCablePacket),
    /// A newer lifecycle state. Lifecycle always wins over buffered packets.
    Lifecycle(LinkCableDataPlaneSnapshot),
}

/// Link-only admission, lifecycle, or relay failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum LinkCableDataPlaneError {
    /// A queue with no storage cannot preserve required link events.
    #[error("link cable queue capacity must be positive")]
    InvalidCapacity,
    /// An authoritative epoch exceeded the signed-63-bit SBLK v1 domain.
    #[error("link cable epoch exceeds the signed-63-bit range")]
    EpochOutOfRange,
    /// A player index was not one of the two link endpoints.
    #[error("link cable player index must be slot 0 or 1")]
    InvalidPlayerIndex,
    /// A connection was already bound to the other endpoint.
    #[error("link cable connection is already bound")]
    ConnectionAlreadyBound,
    /// The requested endpoint still belongs to another connection.
    #[error("link cable endpoint is already bound")]
    EndpointAlreadyBound,
    /// The endpoint's one receiver was already claimed.
    #[error("link cable endpoint receiver was already claimed")]
    ReceiverAlreadyClaimed,
    /// No endpoint is currently bound to the supplied connection.
    #[error("link cable connection is not attached")]
    ConnectionNotAttached,
    /// The receiver's attachment was invalidated or replaced.
    #[error("link cable receiver attachment was replaced")]
    AttachmentReplaced,
    /// The provider cannot accept new work after close.
    #[error("link cable data plane is closed")]
    Closed,
    /// Link traffic arrived before a live two-endpoint cable existed.
    #[error("link cable data plane is not active")]
    NotActive,
    /// The authenticated route supplied a slot other than the bound slot.
    #[error("link cable authenticated slot does not own the connection")]
    AuthenticatedSlotMismatch,
    /// The route's observed room generation was not authoritative.
    #[error("link cable room epoch is stale or unknown")]
    RoomEpochMismatch,
    /// The route's observed provider generation was not authoritative.
    #[error("link cable session epoch is stale or unknown")]
    SessionEpochMismatch,
    /// The JSON envelope attempted to name another player slot.
    #[error("link cable packet envelope slot does not match its authenticated route")]
    EnvelopeSlotMismatch(PlayerIndex),
    /// The JSON envelope's sequence disagreed with the SBLK header.
    #[error("link cable packet envelope sequence does not match its SBLK header")]
    EnvelopeSequenceMismatch,
    /// The JSON envelope's diagnostic time disagreed with a timestamped event.
    #[error("link cable packet envelope time does not match its SBLK event")]
    EnvelopeTimeMismatch,
    /// A decoded SBLK routing field did not match the authoritative attachment.
    #[error("link cable SBLK routing identity does not match the live attachment")]
    WireIdentityMismatch,
    /// Sender sequence was not the exact next value starting at zero.
    #[error("link cable sender sequence is not the exact next value")]
    SenderSequenceMismatch,
    /// The complete SBLK frame fell outside the frozen v1 bounds.
    #[error("link cable SBLK frame size is invalid")]
    InvalidPacketSize,
    /// The selected SBLK namespace rejected the frame.
    #[error("link cable SBLK frame is malformed: {0}")]
    WireCodec(LinkCableWireCodecError),
    /// The decoded event violated the live cable's transaction state machine.
    #[error("link cable transaction is invalid: {0}")]
    Transaction(LinkCableTransactionError),
    /// A required event had no live peer destination.
    #[error("link cable peer endpoint is unavailable")]
    TargetUnavailable,
    /// A required event could not enter its bounded target queue.
    #[error("link cable target queue is full")]
    QueueOverflow,
    /// The signed-63-bit cable generation domain was exhausted.
    #[error("link cable generation is exhausted")]
    CableEpochExhausted,
    /// A process-lifetime attachment identity could no longer be advanced.
    #[error("link cable attachment generation is exhausted")]
    AttachmentGenerationExhausted,
    /// A lifecycle revision could no longer advance without reuse.
    #[error("link cable lifecycle revision is exhausted")]
    LifecycleRevisionExhausted,
    /// A panic occurred while the private data-plane state lock was held.
    #[error("link cable data-plane state is poisoned")]
    StatePoisoned,
}

/// Receiver and initial private state returned by a one-step attachment.
pub struct LinkCableAttachment {
    pub(crate) receiver: LinkCableDataPlaneReceiver,
    pub(crate) snapshot: LinkCableDataPlaneSnapshot,
}

/// Cloneable per-room handle intended to be copied out of the room registry.
#[derive(Clone)]
pub(crate) struct LinkCableDataPlaneHandle {
    inner: Arc<LinkCableDataPlaneInner>,
}

impl fmt::Debug for LinkCableDataPlaneHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LinkCableDataPlaneHandle(<private>)")
    }
}

struct LinkCableDataPlaneInner {
    state: Mutex<LinkCableDataPlaneState>,
    notifiers: [Arc<Notify>; LINK_PLAYER_COUNT],
}

struct LinkCableDataPlaneState {
    room_scope: RoomScope,
    protocol: LinkCableWireProtocol,
    room_epoch: u64,
    session_epoch: u64,
    cable_epoch: u64,
    endpoints: [Option<LinkCableEndpoint>; LINK_PLAYER_COUNT],
    queues: [VecDeque<LinkCablePacket>; LINK_PLAYER_COUNT],
    next_sender_sequences: [u64; LINK_PLAYER_COUNT],
    transaction_state: LinkCableTransactionState,
    queue_capacity: usize,
    status: LinkCableDataPlaneStatus,
    abort_reason: Option<LinkCableAbortReason>,
    lifecycle_revision: u64,
    next_attachment_generation: u64,
}

#[derive(Clone, Copy)]
struct LinkCableEndpoint {
    connection_id: ConnectionId,
    attachment_generation: u64,
    receiver_claimed: bool,
}

/// Single-consumer receiver for one exact connection attachment.
pub struct LinkCableDataPlaneReceiver {
    inner: Weak<LinkCableDataPlaneInner>,
    notify: Arc<Notify>,
    connection_id: ConnectionId,
    local_slot: PlayerIndex,
    attachment_generation: u64,
    observed_lifecycle_revision: u64,
}

impl LinkCableDataPlaneHandle {
    /// Allocates one independent bounded data plane.
    pub(crate) fn new(
        room_scope: RoomScope,
        protocol: LinkCableWireProtocol,
        room_epoch: u64,
        session_epoch: u64,
        queue_capacity: usize,
    ) -> Result<Self, LinkCableDataPlaneError> {
        validate_epoch(room_epoch)?;
        validate_epoch(session_epoch)?;
        if queue_capacity == 0 {
            return Err(LinkCableDataPlaneError::InvalidCapacity);
        }

        Ok(Self {
            inner: Arc::new(LinkCableDataPlaneInner {
                state: Mutex::new(LinkCableDataPlaneState {
                    room_scope,
                    protocol,
                    room_epoch,
                    session_epoch,
                    cable_epoch: 0,
                    endpoints: [None, None],
                    queues: [
                        VecDeque::with_capacity(queue_capacity),
                        VecDeque::with_capacity(queue_capacity),
                    ],
                    next_sender_sequences: [0, 0],
                    transaction_state: LinkCableTransactionState::new(protocol),
                    queue_capacity,
                    status: LinkCableDataPlaneStatus::Waiting,
                    abort_reason: None,
                    lifecycle_revision: 0,
                    next_attachment_generation: 1,
                }),
                notifiers: [Arc::new(Notify::new()), Arc::new(Notify::new())],
            }),
        })
    }

    /// Binds an authenticated room connection without implicitly replacing an
    /// existing endpoint. The WebSocket session subsequently claims its one
    /// receiver with [`Self::claim_receiver`].
    pub(crate) fn bind_connection(
        &self,
        player_index: PlayerIndex,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, LinkCableDataPlaneError> {
        let slot = player_slot(player_index)?;
        let mut state = self.inner.lock()?;
        ensure_open(&state)?;

        if let Some(bound_slot) = connection_slot(&state, connection_id) {
            if bound_slot == slot {
                return Ok(snapshot_for(&state, player_index));
            }
            return Err(LinkCableDataPlaneError::ConnectionAlreadyBound);
        }
        if state.endpoints[slot].is_some() {
            return Err(LinkCableDataPlaneError::EndpointAlreadyBound);
        }

        let attachment_generation = state.next_attachment_generation;
        state.next_attachment_generation = state
            .next_attachment_generation
            .checked_add(1)
            .ok_or(LinkCableDataPlaneError::AttachmentGenerationExhausted)?;
        state.endpoints[slot] = Some(LinkCableEndpoint {
            connection_id,
            attachment_generation,
            receiver_claimed: false,
        });
        state.status = LinkCableDataPlaneStatus::Waiting;
        state.abort_reason = None;
        if let Err(error) = advance_lifecycle(&mut state) {
            drop(state);
            self.inner.notify_both();
            return Err(error);
        }
        let snapshot = snapshot_for(&state, player_index);
        drop(state);
        self.inner.notify_both();
        Ok(snapshot)
    }

    /// Atomically replaces one room-authorized connection binding.
    ///
    /// Runner handoff can claim a slot before the provisional WebSocket closes.
    /// Replacing the endpoint in one critical section invalidates the old
    /// receiver, clears every in-flight packet, and leaves the new connection
    /// bound but unclaimed. No room token or slot state needs to change unless
    /// this operation succeeds.
    pub(crate) fn replace_connection(
        &self,
        player_index: PlayerIndex,
        previous_connection_id: ConnectionId,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, LinkCableDataPlaneError> {
        let slot = player_slot(player_index)?;
        let mut state = self.inner.lock()?;
        ensure_open(&state)?;

        if previous_connection_id == connection_id {
            let endpoint =
                state.endpoints[slot].ok_or(LinkCableDataPlaneError::ConnectionNotAttached)?;
            if endpoint.connection_id != connection_id {
                return Err(LinkCableDataPlaneError::EndpointAlreadyBound);
            }
            return Ok(snapshot_for(&state, player_index));
        }

        if let Some(bound_slot) = connection_slot(&state, connection_id) {
            if bound_slot == slot {
                return Ok(snapshot_for(&state, player_index));
            }
            return Err(LinkCableDataPlaneError::ConnectionAlreadyBound);
        }

        match state.endpoints[slot] {
            Some(endpoint) if endpoint.connection_id == previous_connection_id => {}
            Some(_) => return Err(LinkCableDataPlaneError::EndpointAlreadyBound),
            None => {
                if connection_slot(&state, previous_connection_id).is_some() {
                    return Err(LinkCableDataPlaneError::ConnectionAlreadyBound);
                }
            }
        }

        let attachment_generation = state.next_attachment_generation;
        let next_attachment_generation = attachment_generation
            .checked_add(1)
            .ok_or(LinkCableDataPlaneError::AttachmentGenerationExhausted)?;
        let next_lifecycle_revision = state
            .lifecycle_revision
            .checked_add(1)
            .ok_or(LinkCableDataPlaneError::LifecycleRevisionExhausted)?;

        state.next_attachment_generation = next_attachment_generation;
        state.endpoints[slot] = Some(LinkCableEndpoint {
            connection_id,
            attachment_generation,
            receiver_claimed: false,
        });
        clear_packet_state(&mut state);
        state.status = LinkCableDataPlaneStatus::Waiting;
        state.abort_reason = None;
        state.lifecycle_revision = next_lifecycle_revision;
        let snapshot = snapshot_for(&state, player_index);
        drop(state);
        self.inner.notify_both();
        Ok(snapshot)
    }

    /// Claims the one queue consumer for an already-bound connection.
    pub(crate) fn claim_receiver(
        &self,
        connection_id: ConnectionId,
    ) -> Result<LinkCableAttachment, LinkCableDataPlaneError> {
        let mut state = self.inner.lock()?;
        ensure_open(&state)?;
        let slot = connection_slot(&state, connection_id)
            .ok_or(LinkCableDataPlaneError::ConnectionNotAttached)?;
        let local_slot = player_index(slot);
        let endpoint = state.endpoints[slot].expect("connection lookup returned an endpoint");
        if endpoint.receiver_claimed {
            return Err(LinkCableDataPlaneError::ReceiverAlreadyClaimed);
        }

        let other_slot = 1 - slot;
        let activates =
            state.endpoints[other_slot].is_some_and(|candidate| candidate.receiver_claimed);
        if activates && state.cable_epoch >= MAX_SIGNED_U64 {
            return Err(LinkCableDataPlaneError::CableEpochExhausted);
        }

        state.endpoints[slot]
            .as_mut()
            .expect("claimed endpoint remains attached")
            .receiver_claimed = true;
        let lifecycle_result = if activates {
            activate_new_cable(&mut state)
        } else {
            state.status = LinkCableDataPlaneStatus::Waiting;
            state.abort_reason = None;
            advance_lifecycle(&mut state)
        };
        if let Err(error) = lifecycle_result {
            drop(state);
            self.inner.notify_both();
            return Err(error);
        }

        let endpoint = state.endpoints[slot].expect("claimed endpoint remains attached");
        let snapshot = snapshot_for(&state, local_slot);
        let receiver = LinkCableDataPlaneReceiver {
            inner: Arc::downgrade(&self.inner),
            notify: Arc::clone(&self.inner.notifiers[slot]),
            connection_id,
            local_slot,
            attachment_generation: endpoint.attachment_generation,
            observed_lifecycle_revision: state.lifecycle_revision,
        };
        drop(state);
        self.inner.notify_both();

        Ok(LinkCableAttachment { receiver, snapshot })
    }

    /// Convenience attachment for callers that bind and claim in one step.
    ///
    /// This method preserves the same no-replacement and single-claim rules as
    /// the two-phase API.
    #[cfg(test)]
    pub(crate) fn attach(
        &self,
        player_index: PlayerIndex,
        connection_id: ConnectionId,
    ) -> Result<LinkCableAttachment, LinkCableDataPlaneError> {
        self.bind_connection(player_index, connection_id)?;
        self.claim_receiver(connection_id)
    }

    /// Invalidates one exact connection binding and aborts all queued work.
    pub(crate) fn invalidate_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, LinkCableDataPlaneError> {
        let mut state = self.inner.lock()?;
        ensure_open(&state)?;
        let slot = connection_slot(&state, connection_id)
            .ok_or(LinkCableDataPlaneError::ConnectionNotAttached)?;
        state.endpoints[slot] = None;
        if let Err(error) = abort_cable(&mut state, Some(LinkCableAbortReason::PeerDisconnected)) {
            drop(state);
            self.inner.notify_both();
            return Err(error);
        }
        let snapshot = snapshot_for(&state, player_index(slot));
        drop(state);
        self.inner.notify_both();
        Ok(snapshot)
    }

    /// Updates authoritative room/provider epochs without rotating room scope.
    ///
    /// Existing bindings stay identifiable so the surviving endpoint can
    /// observe the abort. At least one exact connection must be invalidated and
    /// reattached before another cable generation can become active.
    pub(crate) fn synchronize_epochs(
        &self,
        room_epoch: u64,
        session_epoch: u64,
    ) -> Result<(), LinkCableDataPlaneError> {
        validate_epoch(room_epoch)?;
        validate_epoch(session_epoch)?;
        let mut state = self.inner.lock()?;
        ensure_open(&state)?;
        if state.room_epoch == room_epoch && state.session_epoch == session_epoch {
            return Ok(());
        }

        state.room_epoch = room_epoch;
        state.session_epoch = session_epoch;
        if let Err(error) = abort_cable(&mut state, None) {
            drop(state);
            self.inner.notify_both();
            return Err(error);
        }
        drop(state);
        self.inner.notify_both();
        Ok(())
    }

    /// Replaces the provider contract while preserving the stable room scope
    /// and monotonically increasing cable-generation floor.
    #[cfg(test)]
    pub(crate) fn reset_provider(
        &self,
        protocol: LinkCableWireProtocol,
        room_epoch: u64,
        session_epoch: u64,
    ) -> Result<(), LinkCableDataPlaneError> {
        validate_epoch(room_epoch)?;
        validate_epoch(session_epoch)?;
        let mut state = self.inner.lock()?;
        ensure_open(&state)?;
        state.protocol = protocol;
        state.room_epoch = room_epoch;
        state.session_epoch = session_epoch;
        state.endpoints = [None, None];
        if let Err(error) = abort_cable(&mut state, Some(LinkCableAbortReason::CoreClosed)) {
            drop(state);
            self.inner.notify_both();
            return Err(error);
        }
        drop(state);
        self.inner.notify_both();
        Ok(())
    }

    /// Permanently closes the data plane and wakes both endpoint receivers.
    pub(crate) fn close(&self) -> Result<(), LinkCableDataPlaneError> {
        let mut state = self.inner.lock()?;
        if state.status == LinkCableDataPlaneStatus::Closed {
            return Ok(());
        }
        clear_packet_state(&mut state);
        state.status = LinkCableDataPlaneStatus::Closed;
        state.abort_reason = Some(LinkCableAbortReason::CoreClosed);
        if let Err(error) = advance_lifecycle(&mut state) {
            drop(state);
            self.inner.notify_both();
            return Err(error);
        }
        drop(state);
        self.inner.notify_both();
        Ok(())
    }

    /// Returns the current private state projected for one authenticated slot.
    #[cfg(test)]
    pub(crate) fn snapshot(
        &self,
        local_slot: PlayerIndex,
    ) -> Result<LinkCableDataPlaneSnapshot, LinkCableDataPlaneError> {
        player_slot(local_slot)?;
        let state = self.inner.lock()?;
        Ok(snapshot_for(&state, local_slot))
    }

    /// Validates and forwards one required SBLK event to the opposite slot.
    ///
    /// The caller is expected to obtain this handle while holding the room
    /// registry lock and invoke `relay` only after that lock has been released.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn relay(
        &self,
        connection_id: ConnectionId,
        authenticated_slot: PlayerIndex,
        observed_room_epoch: u64,
        observed_session_epoch: u64,
        packet: LinkCablePacket,
    ) -> Result<(), LinkCableDataPlaneError> {
        let sender_slot = player_slot(authenticated_slot)?;
        let mut state = self.inner.lock()?;
        ensure_open(&state)?;
        if state.status != LinkCableDataPlaneStatus::Active {
            return Err(LinkCableDataPlaneError::NotActive);
        }

        let Some(endpoint) = state.endpoints[sender_slot] else {
            return Err(LinkCableDataPlaneError::AuthenticatedSlotMismatch);
        };
        if endpoint.connection_id != connection_id || !endpoint.receiver_claimed {
            return Err(LinkCableDataPlaneError::AuthenticatedSlotMismatch);
        }
        if observed_room_epoch != state.room_epoch {
            return Err(LinkCableDataPlaneError::RoomEpochMismatch);
        }
        if observed_session_epoch != state.session_epoch {
            return Err(LinkCableDataPlaneError::SessionEpochMismatch);
        }
        if packet.player_index != authenticated_slot {
            return self.abort_relay(
                state,
                LinkCableAbortReason::ProtocolViolation,
                LinkCableDataPlaneError::EnvelopeSlotMismatch(packet.player_index),
            );
        }
        if !(LINK_CABLE_WIRE_HEADER_BYTES..=MAX_LINK_CABLE_WIRE_BYTES)
            .contains(&packet.payload.len())
        {
            return self.abort_relay(
                state,
                LinkCableAbortReason::ProtocolViolation,
                LinkCableDataPlaneError::InvalidPacketSize,
            );
        }

        let frame = match decode_link_cable_wire_frame(state.protocol, &packet.payload) {
            Ok(frame) => frame,
            Err(error) => {
                return self.abort_relay(
                    state,
                    LinkCableAbortReason::ProtocolViolation,
                    LinkCableDataPlaneError::WireCodec(error),
                );
            }
        };
        let header = frame.header();
        if header.room_epoch != state.room_epoch
            || header.session_epoch != state.session_epoch
            || header.cable_epoch != state.cable_epoch
            || usize::from(header.sender_slot) != sender_slot
        {
            return self.abort_relay(
                state,
                LinkCableAbortReason::ProtocolViolation,
                LinkCableDataPlaneError::WireIdentityMismatch,
            );
        }
        if header.sender_sequence != packet.sequence {
            return self.abort_relay(
                state,
                LinkCableAbortReason::ProtocolViolation,
                LinkCableDataPlaneError::EnvelopeSequenceMismatch,
            );
        }
        if event_emulated_time(&frame).is_some_and(|time| time != packet.emulated_time) {
            return self.abort_relay(
                state,
                LinkCableAbortReason::ProtocolViolation,
                LinkCableDataPlaneError::EnvelopeTimeMismatch,
            );
        }
        if header.sender_sequence != state.next_sender_sequences[sender_slot] {
            return self.abort_relay(
                state,
                LinkCableAbortReason::ProtocolViolation,
                LinkCableDataPlaneError::SenderSequenceMismatch,
            );
        }

        let target_slot = 1 - sender_slot;
        if !state.endpoints[target_slot].is_some_and(|target| target.receiver_claimed) {
            return self.abort_relay(
                state,
                LinkCableAbortReason::PeerDisconnected,
                LinkCableDataPlaneError::TargetUnavailable,
            );
        }
        if state.queues[target_slot].len() >= state.queue_capacity {
            return self.abort_relay(
                state,
                LinkCableAbortReason::QueueOverflow,
                LinkCableDataPlaneError::QueueOverflow,
            );
        }
        if let Err(error) = state.transaction_state.validate_and_apply(&frame) {
            return self.abort_relay(
                state,
                LinkCableAbortReason::ProtocolViolation,
                LinkCableDataPlaneError::Transaction(error),
            );
        }

        state.queues[target_slot].push_back(packet);
        state.next_sender_sequences[sender_slot] = header
            .sender_sequence
            .checked_add(1)
            .expect("SBLK signed-63-bit sequence always has a u64 successor");
        drop(state);
        self.inner.notifiers[target_slot].notify_one();
        Ok(())
    }

    fn abort_relay(
        &self,
        mut state: MutexGuard<'_, LinkCableDataPlaneState>,
        reason: LinkCableAbortReason,
        error: LinkCableDataPlaneError,
    ) -> Result<(), LinkCableDataPlaneError> {
        if let Err(lifecycle_error) = abort_cable(&mut state, Some(reason)) {
            drop(state);
            self.inner.notify_both();
            return Err(lifecycle_error);
        }
        drop(state);
        self.inner.notify_both();
        Err(error)
    }
}

impl LinkCableDataPlaneReceiver {
    /// Waits for the next lifecycle revision or targeted peer packet.
    pub(crate) async fn recv(
        &mut self,
    ) -> Result<LinkCableDataPlaneEvent, LinkCableDataPlaneError> {
        loop {
            let inner = self
                .inner
                .upgrade()
                .ok_or(LinkCableDataPlaneError::Closed)?;
            {
                let mut state = inner.lock()?;
                let slot = player_slot(self.local_slot)?;
                let Some(endpoint) = state.endpoints[slot] else {
                    return Err(LinkCableDataPlaneError::AttachmentReplaced);
                };
                if endpoint.connection_id != self.connection_id
                    || endpoint.attachment_generation != self.attachment_generation
                    || !endpoint.receiver_claimed
                {
                    return Err(LinkCableDataPlaneError::AttachmentReplaced);
                }

                if state.lifecycle_revision != self.observed_lifecycle_revision {
                    self.observed_lifecycle_revision = state.lifecycle_revision;
                    return Ok(LinkCableDataPlaneEvent::Lifecycle(snapshot_for(
                        &state,
                        self.local_slot,
                    )));
                }
                if state.status == LinkCableDataPlaneStatus::Closed {
                    return Err(LinkCableDataPlaneError::Closed);
                }
                if let Some(packet) = state.queues[slot].pop_front() {
                    return Ok(LinkCableDataPlaneEvent::Packet(packet));
                }
            }

            // notify_one stores a permit when no task is currently waiting, so
            // checking state before awaiting cannot lose a lifecycle or packet
            // wakeup. A stale permit merely causes one harmless loop.
            self.notify.notified().await;
        }
    }
}

impl Drop for LinkCableDataPlaneReceiver {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        inner.release_receiver(
            self.local_slot,
            self.connection_id,
            self.attachment_generation,
        );
    }
}

impl LinkCableDataPlaneInner {
    fn lock(&self) -> Result<MutexGuard<'_, LinkCableDataPlaneState>, LinkCableDataPlaneError> {
        self.state
            .lock()
            .map_err(|_| LinkCableDataPlaneError::StatePoisoned)
    }

    fn notify_both(&self) {
        for notify in &self.notifiers {
            notify.notify_one();
        }
    }

    fn release_receiver(
        &self,
        local_slot: PlayerIndex,
        connection_id: ConnectionId,
        attachment_generation: u64,
    ) {
        let Ok(slot) = player_slot(local_slot) else {
            return;
        };
        let Ok(mut state) = self.lock() else {
            self.notify_both();
            return;
        };
        if state.status == LinkCableDataPlaneStatus::Closed {
            return;
        }
        let Some(endpoint) = state.endpoints[slot] else {
            return;
        };
        if endpoint.connection_id != connection_id
            || endpoint.attachment_generation != attachment_generation
        {
            return;
        }

        state.endpoints[slot] = None;
        let _ = abort_cable(&mut state, Some(LinkCableAbortReason::PeerDisconnected));
        drop(state);
        self.notify_both();
    }
}

fn player_slot(player_index: PlayerIndex) -> Result<usize, LinkCableDataPlaneError> {
    let slot = usize::from(player_index.zero_based());
    if slot < LINK_PLAYER_COUNT {
        Ok(slot)
    } else {
        Err(LinkCableDataPlaneError::InvalidPlayerIndex)
    }
}

fn player_index(slot: usize) -> PlayerIndex {
    PlayerIndex::new(slot as u8, LINK_PLAYER_COUNT as u8)
        .expect("two-slot data plane produced a valid player index")
}

fn connection_slot(state: &LinkCableDataPlaneState, connection_id: ConnectionId) -> Option<usize> {
    state.endpoints.iter().position(|endpoint| {
        endpoint.is_some_and(|endpoint| endpoint.connection_id == connection_id)
    })
}

fn validate_epoch(epoch: u64) -> Result<(), LinkCableDataPlaneError> {
    if epoch > MAX_SIGNED_U64 {
        Err(LinkCableDataPlaneError::EpochOutOfRange)
    } else {
        Ok(())
    }
}

fn ensure_open(state: &LinkCableDataPlaneState) -> Result<(), LinkCableDataPlaneError> {
    if state.status == LinkCableDataPlaneStatus::Closed {
        Err(LinkCableDataPlaneError::Closed)
    } else {
        Ok(())
    }
}

fn activate_new_cable(state: &mut LinkCableDataPlaneState) -> Result<(), LinkCableDataPlaneError> {
    debug_assert!(state.cable_epoch < MAX_SIGNED_U64);
    state.cable_epoch += 1;
    clear_packet_state(state);
    state.status = LinkCableDataPlaneStatus::Active;
    state.abort_reason = None;
    advance_lifecycle(state)
}

fn abort_cable(
    state: &mut LinkCableDataPlaneState,
    abort_reason: Option<LinkCableAbortReason>,
) -> Result<(), LinkCableDataPlaneError> {
    clear_packet_state(state);
    state.status = LinkCableDataPlaneStatus::Aborted;
    state.abort_reason = abort_reason;
    advance_lifecycle(state)
}

fn clear_packet_state(state: &mut LinkCableDataPlaneState) {
    for queue in &mut state.queues {
        queue.clear();
    }
    state.next_sender_sequences = [0, 0];
    state.transaction_state.reset(state.protocol);
}

fn advance_lifecycle(state: &mut LinkCableDataPlaneState) -> Result<(), LinkCableDataPlaneError> {
    let Some(next_revision) = state.lifecycle_revision.checked_add(1) else {
        clear_packet_state(state);
        state.status = LinkCableDataPlaneStatus::Closed;
        state.abort_reason = Some(LinkCableAbortReason::CoreClosed);
        return Err(LinkCableDataPlaneError::LifecycleRevisionExhausted);
    };
    state.lifecycle_revision = next_revision;
    Ok(())
}

fn snapshot_for(
    state: &LinkCableDataPlaneState,
    local_slot: PlayerIndex,
) -> LinkCableDataPlaneSnapshot {
    LinkCableDataPlaneSnapshot {
        room_scope: state.room_scope.get(),
        room_epoch: state.room_epoch,
        session_epoch: state.session_epoch,
        cable_epoch: state.cable_epoch,
        queue_capacity: state.queue_capacity,
        protocol: state.protocol,
        status: state.status,
        abort_reason: state.abort_reason,
        lifecycle_revision: state.lifecycle_revision,
        local_slot,
    }
}

fn event_emulated_time(frame: &LinkCableWireFrame) -> Option<u64> {
    match frame {
        LinkCableWireFrame::GbaSioMulti(frame) => match &frame.event {
            GbaSioMultiEvent::ModeSet { emulated_time, .. }
            | GbaSioMultiEvent::TransferStart { emulated_time, .. }
            | GbaSioMultiEvent::TransferReply { emulated_time, .. } => Some(*emulated_time),
            GbaSioMultiEvent::TransferCommit { .. } | GbaSioMultiEvent::TransferAbort { .. } => {
                None
            }
        },
        LinkCableWireFrame::GbSerial(frame) => match &frame.event {
            GbSerialEvent::Start { emulated_time, .. }
            | GbSerialEvent::Reply { emulated_time, .. } => Some(*emulated_time),
            GbSerialEvent::Commit { .. } | GbSerialEvent::Abort { .. } => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LinkCableAttachment, LinkCableDataPlaneError, LinkCableDataPlaneEvent,
        LinkCableDataPlaneHandle, LinkCableDataPlaneReceiver, LinkCableDataPlaneStatus,
    };
    use crate::protocol::{
        GB_SERIAL_NORMAL_CLOCK_CONTROL, GbSerialEvent, GbSerialFrame, GbaSioMultiEvent,
        GbaSioMultiFrame, LinkCableAbortReason, LinkCablePacket, LinkCableWireCodecError,
        LinkCableWireHeader, LinkCableWireProtocol, encode_gb_serial_frame,
        encode_gba_sio_multi_frame,
    };
    use crate::rooms::{ConnectionId, LinkCableTransactionError, PlayerIndex, RoomScope};
    use std::time::Duration;

    const ROOM_EPOCH: u64 = 11;
    const SESSION_EPOCH: u64 = 17;
    const EMULATED_TIME: u64 = 23;

    #[tokio::test]
    async fn relays_only_to_the_opposite_endpoint_without_echo() {
        let handle = data_plane(4);
        let connection_zero = ConnectionId::new();
        let connection_one = ConnectionId::new();
        let LinkCableAttachment {
            receiver: mut receiver_zero,
            snapshot: waiting,
        } = handle
            .attach(PlayerIndex::ONE, connection_zero)
            .expect("attach slot zero");
        assert_eq!(waiting.status, LinkCableDataPlaneStatus::Waiting);
        let LinkCableAttachment {
            receiver: mut receiver_one,
            snapshot: active,
        } = handle
            .attach(PlayerIndex::TWO, connection_one)
            .expect("attach slot one");
        assert_eq!(active.status, LinkCableDataPlaneStatus::Active);

        assert_lifecycle(&mut receiver_zero, LinkCableDataPlaneStatus::Active).await;
        let packet = packet(PlayerIndex::ONE, 0, active.cable_epoch, EMULATED_TIME);
        handle
            .relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                packet.clone(),
            )
            .expect("relay packet");

        assert_eq!(
            receiver_one.recv().await.expect("targeted packet"),
            LinkCableDataPlaneEvent::Packet(packet)
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(20), receiver_zero.recv())
                .await
                .is_err(),
            "sender must not receive its own packet"
        );
    }

    #[tokio::test]
    async fn requires_exact_sender_sequence_starting_at_zero() {
        let (handle, connection_zero, _connection_one, _receiver_zero, mut receiver_one, cable) =
            active_pair(4).await;

        handle
            .relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                packet(PlayerIndex::ONE, 0, cable, EMULATED_TIME),
            )
            .expect("sequence zero");
        assert_eq!(
            handle.relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                packet(PlayerIndex::ONE, 2, cable, EMULATED_TIME),
            ),
            Err(LinkCableDataPlaneError::SenderSequenceMismatch)
        );

        let lifecycle =
            assert_lifecycle(&mut receiver_one, LinkCableDataPlaneStatus::Aborted).await;
        assert_eq!(
            lifecycle.abort_reason,
            Some(crate::protocol::LinkCableAbortReason::ProtocolViolation)
        );
    }

    #[tokio::test]
    async fn stale_spoofed_and_malformed_frames_fail_closed() {
        let (handle, connection_zero, _connection_one, _receiver_zero, _receiver_one, cable) =
            active_pair(2).await;
        assert_eq!(
            handle.relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH - 1,
                SESSION_EPOCH,
                packet(PlayerIndex::ONE, 0, cable, EMULATED_TIME),
            ),
            Err(LinkCableDataPlaneError::RoomEpochMismatch)
        );
        assert_eq!(
            handle.relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                packet(PlayerIndex::TWO, 0, cable, EMULATED_TIME),
            ),
            Err(LinkCableDataPlaneError::EnvelopeSlotMismatch(
                PlayerIndex::TWO
            ))
        );

        let (
            malformed_handle,
            malformed_connection,
            _malformed_peer_connection,
            _malformed_sender_receiver,
            _malformed_target_receiver,
            _malformed_cable,
        ) = active_pair(2).await;
        let malformed = LinkCablePacket {
            player_index: PlayerIndex::ONE,
            sequence: 0,
            emulated_time: 0,
            payload: vec![0; 43],
        };
        assert_eq!(
            malformed_handle.relay(
                malformed_connection,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                malformed,
            ),
            Err(LinkCableDataPlaneError::WireCodec(
                LinkCableWireCodecError::UnsupportedMagic
            ))
        );
        assert_eq!(
            malformed_handle
                .snapshot(PlayerIndex::ONE)
                .expect("snapshot")
                .status,
            LinkCableDataPlaneStatus::Aborted
        );
    }

    #[tokio::test]
    async fn overflow_clears_queues_atomically_and_lifecycle_precedes_delivery() {
        let (handle, connection_zero, _connection_one, _receiver_zero, mut receiver_one, cable) =
            active_pair(1).await;
        handle
            .relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                packet(PlayerIndex::ONE, 0, cable, EMULATED_TIME),
            )
            .expect("fill queue");
        assert_eq!(
            handle.relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                packet(PlayerIndex::ONE, 1, cable, EMULATED_TIME + 1),
            ),
            Err(LinkCableDataPlaneError::QueueOverflow)
        );

        let lifecycle =
            assert_lifecycle(&mut receiver_one, LinkCableDataPlaneStatus::Aborted).await;
        assert_eq!(
            lifecycle.abort_reason,
            Some(crate::protocol::LinkCableAbortReason::QueueOverflow)
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(20), receiver_one.recv())
                .await
                .is_err(),
            "the packet buffered before overflow must be cleared"
        );
    }

    #[tokio::test]
    async fn gba_transaction_violation_aborts_and_clears_both_target_queues() {
        let (handle, connection_zero, connection_one, mut receiver_zero, mut receiver_one, cable) =
            active_pair(8).await;

        handle
            .relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                gba_packet(
                    PlayerIndex::ONE,
                    0,
                    cable,
                    GbaSioMultiEvent::ModeSet {
                        mode: 2,
                        siocnt: 0x2000,
                        rcnt: 0,
                        emulated_time: 1,
                    },
                    1,
                ),
            )
            .expect("slot zero mode");
        handle
            .relay(
                connection_one,
                PlayerIndex::TWO,
                ROOM_EPOCH,
                SESSION_EPOCH,
                gba_packet(
                    PlayerIndex::TWO,
                    0,
                    cable,
                    GbaSioMultiEvent::ModeSet {
                        mode: 2,
                        siocnt: 0x2000,
                        rcnt: 0,
                        emulated_time: 2,
                    },
                    2,
                ),
            )
            .expect("slot one mode");
        handle
            .relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                gba_packet(
                    PlayerIndex::ONE,
                    1,
                    cable,
                    GbaSioMultiEvent::TransferStart {
                        transfer_id: 1,
                        siocnt: 0x2000,
                        parent_word: 0x1111,
                        emulated_time: 3,
                    },
                    3,
                ),
            )
            .expect("GBA start");
        handle
            .relay(
                connection_one,
                PlayerIndex::TWO,
                ROOM_EPOCH,
                SESSION_EPOCH,
                gba_packet(
                    PlayerIndex::TWO,
                    1,
                    cable,
                    GbaSioMultiEvent::TransferReply {
                        transfer_id: 1,
                        child_word: 0x2222,
                        emulated_time: 4,
                    },
                    4,
                ),
            )
            .expect("GBA reply");

        let result = handle.relay(
            connection_zero,
            PlayerIndex::ONE,
            ROOM_EPOCH,
            SESSION_EPOCH,
            gba_packet(
                PlayerIndex::ONE,
                2,
                cable,
                GbaSioMultiEvent::TransferCommit {
                    transfer_id: 1,
                    words: [0x1111, 0x9999, 0xffff, 0xffff],
                },
                0,
            ),
        );

        assert_eq!(
            result,
            Err(LinkCableDataPlaneError::Transaction(
                LinkCableTransactionError::CommitPayloadMismatch
            ))
        );
        for receiver in [&mut receiver_zero, &mut receiver_one] {
            let lifecycle = assert_lifecycle(receiver, LinkCableDataPlaneStatus::Aborted).await;
            assert_eq!(
                lifecycle.abort_reason,
                Some(LinkCableAbortReason::ProtocolViolation)
            );
            assert!(
                tokio::time::timeout(Duration::from_millis(20), receiver.recv())
                    .await
                    .is_err(),
                "transaction abort must clear every previously queued event"
            );
        }
    }

    #[tokio::test]
    async fn gb_transaction_happy_path_relays_start_reply_and_matching_commit() {
        let (handle, connection_zero, connection_one, mut receiver_zero, mut receiver_one, cable) =
            active_pair_for(4, LinkCableWireProtocol::GbSerialV1).await;
        let start = gb_packet(
            PlayerIndex::ONE,
            0,
            cable,
            GbSerialEvent::Start {
                transfer_id: 1,
                clock_owner_slot: 0,
                sc_control: GB_SERIAL_NORMAL_CLOCK_CONTROL,
                owner_byte: 0xa5,
                emulated_time: 1,
            },
            1,
        );
        handle
            .relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                start.clone(),
            )
            .expect("GB start");
        assert_eq!(
            receiver_one.recv().await.expect("GB start delivery"),
            LinkCableDataPlaneEvent::Packet(start)
        );

        let reply = gb_packet(
            PlayerIndex::TWO,
            0,
            cable,
            GbSerialEvent::Reply {
                transfer_id: 1,
                clock_owner_slot: 0,
                responder_byte: 0x3c,
                emulated_time: 2,
            },
            2,
        );
        handle
            .relay(
                connection_one,
                PlayerIndex::TWO,
                ROOM_EPOCH,
                SESSION_EPOCH,
                reply.clone(),
            )
            .expect("GB reply");
        assert_eq!(
            receiver_zero.recv().await.expect("GB reply delivery"),
            LinkCableDataPlaneEvent::Packet(reply)
        );

        let commit = gb_packet(
            PlayerIndex::ONE,
            1,
            cable,
            GbSerialEvent::Commit {
                transfer_id: 1,
                clock_owner_slot: 0,
                slot_bytes: [0xa5, 0x3c],
            },
            0,
        );
        handle
            .relay(
                connection_zero,
                PlayerIndex::ONE,
                ROOM_EPOCH,
                SESSION_EPOCH,
                commit.clone(),
            )
            .expect("GB commit");
        assert_eq!(
            receiver_one.recv().await.expect("GB commit delivery"),
            LinkCableDataPlaneEvent::Packet(commit)
        );
        assert_eq!(
            handle
                .snapshot(PlayerIndex::ONE)
                .expect("active snapshot")
                .status,
            LinkCableDataPlaneStatus::Active
        );
    }

    #[tokio::test]
    async fn surviving_receiver_continues_after_peer_reconnect_with_newer_epoch() {
        let (
            handle,
            _connection_zero,
            connection_one,
            mut receiver_zero,
            mut old_receiver_one,
            first_cable_epoch,
        ) = active_pair(2).await;

        handle
            .invalidate_connection(connection_one)
            .expect("detach peer");
        assert_lifecycle(&mut receiver_zero, LinkCableDataPlaneStatus::Aborted).await;
        assert_eq!(
            old_receiver_one.recv().await,
            Err(LinkCableDataPlaneError::AttachmentReplaced)
        );

        let replacement_connection = ConnectionId::new();
        let LinkCableAttachment {
            receiver: _replacement_receiver,
            snapshot: replacement_snapshot,
        } = handle
            .attach(PlayerIndex::TWO, replacement_connection)
            .expect("reattach peer");
        assert_eq!(
            replacement_snapshot.status,
            LinkCableDataPlaneStatus::Active
        );
        assert!(replacement_snapshot.cable_epoch > first_cable_epoch);
        assert_lifecycle(&mut receiver_zero, LinkCableDataPlaneStatus::Active).await;

        let packet = packet(
            PlayerIndex::TWO,
            0,
            replacement_snapshot.cable_epoch,
            EMULATED_TIME,
        );
        handle
            .relay(
                replacement_connection,
                PlayerIndex::TWO,
                ROOM_EPOCH,
                SESSION_EPOCH,
                packet.clone(),
            )
            .expect("relay after reconnect");
        assert_eq!(
            receiver_zero.recv().await.expect("packet after reconnect"),
            LinkCableDataPlaneEvent::Packet(packet)
        );
    }

    #[tokio::test]
    async fn room_scope_stays_stable_across_epoch_and_provider_resets() {
        let (
            handle,
            _connection_zero,
            _connection_one,
            _receiver_zero,
            _receiver_one,
            first_cable_epoch,
        ) = active_pair(2).await;
        let original_scope = handle
            .snapshot(PlayerIndex::ONE)
            .expect("original snapshot")
            .room_scope;

        handle
            .synchronize_epochs(ROOM_EPOCH + 1, SESSION_EPOCH + 1)
            .expect("synchronize epochs");
        assert_eq!(
            handle
                .snapshot(PlayerIndex::ONE)
                .expect("synchronized snapshot")
                .room_scope,
            original_scope
        );
        handle
            .reset_provider(
                LinkCableWireProtocol::GbSerialV1,
                ROOM_EPOCH + 2,
                SESSION_EPOCH + 2,
            )
            .expect("reset provider");
        assert_eq!(
            handle
                .snapshot(PlayerIndex::ONE)
                .expect("reset snapshot")
                .room_scope,
            original_scope
        );

        let first = handle
            .attach(PlayerIndex::ONE, ConnectionId::new())
            .expect("new provider slot zero");
        let second = handle
            .attach(PlayerIndex::TWO, ConnectionId::new())
            .expect("new provider slot one");
        assert_eq!(first.snapshot.room_scope, original_scope);
        assert_eq!(second.snapshot.room_scope, original_scope);
        assert!(second.snapshot.cable_epoch > first_cable_epoch);
    }

    #[tokio::test]
    async fn close_wakes_a_waiting_receiver() {
        let handle = data_plane(2);
        let LinkCableAttachment { mut receiver, .. } = handle
            .attach(PlayerIndex::ONE, ConnectionId::new())
            .expect("attach receiver");
        let waiter = tokio::spawn(async move { receiver.recv().await });
        tokio::task::yield_now().await;

        handle.close().expect("close data plane");
        assert!(matches!(
            waiter.await.expect("receiver task"),
            Ok(LinkCableDataPlaneEvent::Lifecycle(snapshot))
                if snapshot.status == LinkCableDataPlaneStatus::Closed
                    && snapshot.abort_reason == Some(LinkCableAbortReason::CoreClosed)
        ));
    }

    #[test]
    fn poisoned_state_is_never_recovered_or_reused() {
        let handle = data_plane(2);
        let inner = std::sync::Arc::clone(&handle.inner);
        let _ = std::panic::catch_unwind(move || {
            let _state = inner.state.lock().expect("initial state lock");
            panic!("poison private data-plane state");
        });

        assert_eq!(
            handle.snapshot(PlayerIndex::ONE),
            Err(LinkCableDataPlaneError::StatePoisoned)
        );
    }

    #[test]
    fn attachment_and_lifecycle_generations_never_saturate_into_reuse() {
        let attachment_handle = data_plane(2);
        {
            let mut state = attachment_handle
                .inner
                .state
                .lock()
                .expect("attachment state");
            state.next_attachment_generation = u64::MAX;
        }
        assert_eq!(
            attachment_handle.bind_connection(PlayerIndex::ONE, ConnectionId::new()),
            Err(LinkCableDataPlaneError::AttachmentGenerationExhausted)
        );

        let lifecycle_handle = data_plane(2);
        {
            let mut state = lifecycle_handle
                .inner
                .state
                .lock()
                .expect("lifecycle state");
            state.lifecycle_revision = u64::MAX;
        }
        assert_eq!(
            lifecycle_handle.bind_connection(PlayerIndex::ONE, ConnectionId::new()),
            Err(LinkCableDataPlaneError::LifecycleRevisionExhausted)
        );
        assert_eq!(
            lifecycle_handle
                .snapshot(PlayerIndex::ONE)
                .expect("closed snapshot")
                .status,
            LinkCableDataPlaneStatus::Closed
        );
    }

    fn data_plane(capacity: usize) -> LinkCableDataPlaneHandle {
        data_plane_for(capacity, LinkCableWireProtocol::GbaSioMultiV1)
    }

    fn data_plane_for(
        capacity: usize,
        protocol: LinkCableWireProtocol,
    ) -> LinkCableDataPlaneHandle {
        LinkCableDataPlaneHandle::new(
            RoomScope::new(101).expect("room scope"),
            protocol,
            ROOM_EPOCH,
            SESSION_EPOCH,
            capacity,
        )
        .expect("data plane")
    }

    async fn active_pair(
        capacity: usize,
    ) -> (
        LinkCableDataPlaneHandle,
        ConnectionId,
        ConnectionId,
        LinkCableDataPlaneReceiver,
        LinkCableDataPlaneReceiver,
        u64,
    ) {
        active_pair_for(capacity, LinkCableWireProtocol::GbaSioMultiV1).await
    }

    async fn active_pair_for(
        capacity: usize,
        protocol: LinkCableWireProtocol,
    ) -> (
        LinkCableDataPlaneHandle,
        ConnectionId,
        ConnectionId,
        LinkCableDataPlaneReceiver,
        LinkCableDataPlaneReceiver,
        u64,
    ) {
        let handle = data_plane_for(capacity, protocol);
        let connection_zero = ConnectionId::new();
        let connection_one = ConnectionId::new();
        let LinkCableAttachment {
            receiver: mut receiver_zero,
            ..
        } = handle
            .attach(PlayerIndex::ONE, connection_zero)
            .expect("attach slot zero");
        let LinkCableAttachment {
            receiver: receiver_one,
            snapshot,
        } = handle
            .attach(PlayerIndex::TWO, connection_one)
            .expect("attach slot one");
        assert_lifecycle(&mut receiver_zero, LinkCableDataPlaneStatus::Active).await;
        (
            handle,
            connection_zero,
            connection_one,
            receiver_zero,
            receiver_one,
            snapshot.cable_epoch,
        )
    }

    fn packet(
        player_index: PlayerIndex,
        sequence: u64,
        cable_epoch: u64,
        emulated_time: u64,
    ) -> LinkCablePacket {
        let sender_slot = player_index.zero_based();
        let payload = encode_gba_sio_multi_frame(&GbaSioMultiFrame {
            header: LinkCableWireHeader {
                room_epoch: ROOM_EPOCH,
                session_epoch: SESSION_EPOCH,
                cable_epoch,
                sender_sequence: sequence,
                sender_slot,
            },
            event: GbaSioMultiEvent::ModeSet {
                mode: 0,
                siocnt: 0,
                rcnt: 0,
                emulated_time,
            },
        })
        .expect("encode test frame");
        LinkCablePacket {
            player_index,
            sequence,
            emulated_time,
            payload,
        }
    }

    fn gba_packet(
        player_index: PlayerIndex,
        sequence: u64,
        cable_epoch: u64,
        event: GbaSioMultiEvent,
        emulated_time: u64,
    ) -> LinkCablePacket {
        let payload = encode_gba_sio_multi_frame(&GbaSioMultiFrame {
            header: test_header(player_index, sequence, cable_epoch),
            event,
        })
        .expect("encode GBA test frame");
        LinkCablePacket {
            player_index,
            sequence,
            emulated_time,
            payload,
        }
    }

    fn gb_packet(
        player_index: PlayerIndex,
        sequence: u64,
        cable_epoch: u64,
        event: GbSerialEvent,
        emulated_time: u64,
    ) -> LinkCablePacket {
        let payload = encode_gb_serial_frame(&GbSerialFrame {
            header: test_header(player_index, sequence, cable_epoch),
            event,
        })
        .expect("encode GB test frame");
        LinkCablePacket {
            player_index,
            sequence,
            emulated_time,
            payload,
        }
    }

    fn test_header(
        player_index: PlayerIndex,
        sequence: u64,
        cable_epoch: u64,
    ) -> LinkCableWireHeader {
        LinkCableWireHeader {
            room_epoch: ROOM_EPOCH,
            session_epoch: SESSION_EPOCH,
            cable_epoch,
            sender_sequence: sequence,
            sender_slot: player_index.zero_based(),
        }
    }

    async fn assert_lifecycle(
        receiver: &mut LinkCableDataPlaneReceiver,
        status: LinkCableDataPlaneStatus,
    ) -> super::LinkCableDataPlaneSnapshot {
        let LinkCableDataPlaneEvent::Lifecycle(snapshot) =
            receiver.recv().await.expect("lifecycle event")
        else {
            panic!("expected lifecycle before packet");
        };
        assert_eq!(snapshot.status, status);
        snapshot
    }
}
