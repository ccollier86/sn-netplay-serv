//! Stored room wrapper with event broadcasting.
//!
//! The registry owns lookup and locking; this wrapper owns the event channel
//! beside a room and exposes small emit helpers. It does not parse protocol
//! messages or apply room-domain rules.

use crate::protocol::{InputFrame, LinkCablePacket, SnapshotChunk, SnapshotManifest};
use crate::rooms::{ConnectionId, NetplayRoom, RoomEvent};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const ROOM_EVENT_CHANNEL_CAPACITY: usize = 32;

/// Room plus event channel stored by the in-memory registry.
pub(super) struct StoredRoom {
    pub(super) room: NetplayRoom,
    events: broadcast::Sender<RoomEvent>,
    created_at: Instant,
}

impl StoredRoom {
    /// Creates a stored room with a bounded event channel.
    pub(super) fn new(room: NetplayRoom) -> Self {
        let (events, _) = broadcast::channel(ROOM_EVENT_CHANNEL_CAPACITY);

        Self {
            room,
            events,
            created_at: Instant::now(),
        }
    }

    /// Returns whether a waiting room has exceeded the join timeout.
    pub(super) fn is_expired_waiting(&self, now: Instant, timeout: Duration) -> bool {
        self.room.status() == crate::rooms::RoomStatus::WaitingForGuest
            && now.duration_since(self.created_at) >= timeout
    }

    /// Subscribes to room events.
    pub(super) fn subscribe(&self) -> broadcast::Receiver<RoomEvent> {
        self.events.subscribe()
    }

    /// Emits the current room view.
    pub(super) fn emit_state(&self) {
        let _ = self
            .events
            .send(RoomEvent::RoomStateChanged(self.room.view()));
    }

    /// Emits a session-start event.
    pub(super) fn emit_start(&self, start_frame: u64) {
        let _ = self.events.send(RoomEvent::SessionStarted {
            start_frame,
            room: self.room.view(),
        });
    }

    /// Emits a validated snapshot chunk.
    pub(super) fn emit_snapshot_chunk(&self, source: ConnectionId, chunk: SnapshotChunk) {
        let _ = self.events.send(RoomEvent::SnapshotChunk { source, chunk });
    }

    /// Emits a validated snapshot manifest.
    pub(super) fn emit_snapshot_complete(&self, source: ConnectionId, manifest: SnapshotManifest) {
        let _ = self
            .events
            .send(RoomEvent::SnapshotComplete { source, manifest });
    }

    /// Emits a validated controller input frame.
    pub(super) fn emit_input_frame(&self, source: ConnectionId, input: InputFrame) {
        let _ = self.events.send(RoomEvent::InputFrame { source, input });
    }

    /// Emits a validated link-cable packet.
    pub(super) fn emit_link_cable_packet(&self, source: ConnectionId, packet: LinkCablePacket) {
        let _ = self
            .events
            .send(RoomEvent::LinkCablePacket { source, packet });
    }
}
