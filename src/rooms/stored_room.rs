//! Stored room wrapper with event broadcasting.
//!
//! The registry owns lookup and locking; this wrapper owns the event channel
//! beside a room and exposes small emit helpers. It does not parse protocol
//! messages or apply room-domain rules.

use crate::protocol::{
    InputFrame, LinkCablePacket, SessionPauseView, SnapshotChunk, SnapshotManifest,
};
use crate::rooms::{
    ConnectionId, NetplayRoom, RoomDebugEvent, RoomDebugEventLog, RoomEvent, RoomView,
    current_timestamp_ms,
};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const ROOM_EVENT_CHANNEL_CAPACITY: usize = 32;

/// Room plus event channel stored by the in-memory registry.
pub(super) struct StoredRoom {
    pub(super) room: NetplayRoom,
    events: broadcast::Sender<RoomEvent>,
    event_seq: u64,
    debug_events: RoomDebugEventLog,
    created_at: Instant,
}

impl StoredRoom {
    /// Creates a stored room with a bounded event channel.
    pub(super) fn new(room: NetplayRoom) -> Self {
        let (events, _) = broadcast::channel(ROOM_EVENT_CHANNEL_CAPACITY);

        Self {
            room,
            events,
            event_seq: 0,
            debug_events: RoomDebugEventLog::default(),
            created_at: Instant::now(),
        }
    }

    /// Returns whether a waiting room has exceeded the join timeout.
    pub(super) fn is_expired_waiting(&self, now: Instant, timeout: Duration) -> bool {
        self.room.status() == crate::rooms::RoomStatus::WaitingForGuest
            && now.duration_since(self.created_at) >= timeout
    }

    /// Returns whether the room's reconnect grace has expired.
    pub(super) fn is_expired_recovery(&self, now: Instant) -> bool {
        self.room.is_recovery_expired(now)
    }

    /// Returns whether every occupied slot has been disconnected long enough.
    pub(super) fn is_idle_disconnected(&self, now: Instant, timeout: Duration) -> bool {
        self.room.is_idle_disconnected(now, timeout)
    }

    /// Starts recovery for connected clients that stopped heartbeating.
    pub(super) fn recover_stale_connections(
        &mut self,
        now: Instant,
        heartbeat_disconnect: Duration,
        reconnect_grace: Duration,
    ) -> bool {
        self.room
            .recover_stale_connections(now, heartbeat_disconnect, reconnect_grace)
    }

    /// Marks connected players whose heartbeat is late but still recoverable.
    pub(super) fn mark_stale_connections(
        &mut self,
        now: Instant,
        heartbeat_stale: Duration,
        heartbeat_disconnect: Duration,
    ) -> bool {
        self.room
            .mark_stale_connections(now, heartbeat_stale, heartbeat_disconnect)
    }

    /// Subscribes to room events.
    pub(super) fn subscribe(&self) -> broadcast::Receiver<RoomEvent> {
        self.events.subscribe()
    }

    /// Returns a room view with the current event sequence.
    pub(super) fn view(&self, now: Instant) -> RoomView {
        self.room.view_for_event(self.event_seq, now)
    }

    /// Returns recent sanitized events for this room.
    pub(super) fn debug_events(&self, limit: usize) -> Vec<RoomDebugEvent> {
        self.debug_events.tail(limit)
    }

    /// Emits the current room view.
    pub(super) fn emit_state(&mut self, now: Instant, kind: &'static str, detail: &'static str) {
        let room = self.record_event(now, kind, detail);
        let _ = self.events.send(RoomEvent::RoomStateChanged(room));
    }

    /// Emits a session-start event.
    pub(super) fn emit_start(&mut self, now: Instant, start_frame: u64) {
        let room = self.record_event(now, "sessionStarted", "session started");
        let _ = self
            .events
            .send(RoomEvent::SessionStarted { start_frame, room });
    }

    /// Emits a coordinated pause schedule.
    pub(super) fn emit_session_pause_scheduled(&mut self, now: Instant, pause: SessionPauseView) {
        let room = self.record_event(now, "pauseScheduled", "coordinated pause scheduled");
        let _ = self
            .events
            .send(RoomEvent::SessionPauseScheduled { pause, room });
    }

    /// Emits a coordinated pause update.
    pub(super) fn emit_session_pause_updated(&mut self, now: Instant, pause: SessionPauseView) {
        let room = self.record_event(now, "pauseUpdated", "coordinated pause updated");
        let _ = self
            .events
            .send(RoomEvent::SessionPauseUpdated { pause, room });
    }

    /// Emits a coordinated resume schedule.
    pub(super) fn emit_session_resume_scheduled(
        &mut self,
        now: Instant,
        sequence: u64,
        resume_at_frame: u64,
    ) {
        let room = self.record_event(now, "resumeScheduled", "coordinated resume scheduled");
        let _ = self.events.send(RoomEvent::SessionResumeScheduled {
            sequence,
            resume_at_frame,
            room,
        });
    }

    /// Emits a validated snapshot chunk.
    pub(super) fn emit_snapshot_chunk(
        &mut self,
        now: Instant,
        source: ConnectionId,
        chunk: SnapshotChunk,
    ) {
        self.record_event(now, "snapshotChunk", "snapshot chunk relayed");
        let _ = self.events.send(RoomEvent::SnapshotChunk { source, chunk });
    }

    /// Emits a validated snapshot manifest.
    pub(super) fn emit_snapshot_complete(
        &mut self,
        now: Instant,
        source: ConnectionId,
        manifest: SnapshotManifest,
    ) {
        self.record_event(now, "snapshotComplete", "snapshot manifest relayed");
        let _ = self
            .events
            .send(RoomEvent::SnapshotComplete { source, manifest });
    }

    /// Emits a validated controller input frame.
    pub(super) fn emit_input_frame(
        &mut self,
        now: Instant,
        source: ConnectionId,
        input: InputFrame,
    ) {
        self.record_event(now, "inputFrame", "input frame relayed");
        let _ = self.events.send(RoomEvent::InputFrame { source, input });
    }

    /// Emits a validated link-cable packet.
    pub(super) fn emit_link_cable_packet(
        &mut self,
        now: Instant,
        source: ConnectionId,
        packet: LinkCablePacket,
    ) {
        self.record_event(now, "linkCablePacket", "link-cable packet relayed");
        let _ = self
            .events
            .send(RoomEvent::LinkCablePacket { source, packet });
    }

    fn record_event(&mut self, now: Instant, kind: &str, detail: &str) -> RoomView {
        self.event_seq = self.event_seq.saturating_add(1);
        let room = self.room.view_for_event(self.event_seq, now);
        self.debug_events.push(RoomDebugEvent {
            timestamp_ms: current_timestamp_ms(),
            room_id: room.room_id,
            invite_code: room.invite_code.clone(),
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            kind: kind.to_string(),
            detail: detail.to_string(),
        });
        room
    }
}
