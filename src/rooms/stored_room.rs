//! Stored room wrapper with event broadcasting.
//!
//! The registry owns lookup and locking; this wrapper owns the event channel
//! beside a room and exposes small emit helpers. It does not parse protocol
//! messages or apply room-domain rules.

use crate::protocol::{
    ClientNetworkQualityReport, ClientRuntimeState, InputDelayChange, InputFrame, InputFrameBatch,
    LinkCablePacket, ServerFrame, SessionPauseView, SnapshotChunk, SnapshotManifest,
    StateHashMismatchView,
};
use crate::rooms::{
    ConnectionId, InputFrameRelayBuffer, NetplayRoom, RoomDebugEvent, RoomDebugEventLog, RoomEvent,
    RoomInputEvent, RoomPerformanceSample, RoomView, current_timestamp_ms,
};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const ROOM_EVENT_CHANNEL_CAPACITY: usize = 512;
const INPUT_EVENT_CHANNEL_CAPACITY: usize = 512;

/// Room plus event channel stored by the in-memory registry.
pub(super) struct StoredRoom {
    pub(super) room: NetplayRoom,
    events: broadcast::Sender<RoomEvent>,
    input_events: broadcast::Sender<RoomInputEvent>,
    input_relay_buffer: InputFrameRelayBuffer,
    event_seq: u64,
    debug_events: RoomDebugEventLog,
    created_at: Instant,
}

impl StoredRoom {
    /// Creates a stored room with a bounded event channel.
    pub(super) fn new(room: NetplayRoom) -> Self {
        let (events, _) = broadcast::channel(ROOM_EVENT_CHANNEL_CAPACITY);
        let (input_events, _) = broadcast::channel(INPUT_EVENT_CHANNEL_CAPACITY);

        Self {
            room,
            events,
            input_events,
            input_relay_buffer: InputFrameRelayBuffer::default(),
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

    /// Subscribes to gameplay input events only.
    pub(super) fn subscribe_input(&self) -> broadcast::Receiver<RoomInputEvent> {
        self.input_events.subscribe()
    }

    /// Returns a room view with the current event sequence.
    pub(super) fn view(&self, now: Instant) -> RoomView {
        self.room.view_for_event(self.event_seq, now)
    }

    /// Builds a sanitized performance sample for one heartbeat.
    pub(super) fn performance_sample(
        &self,
        _now: Instant,
        connection_id: ConnectionId,
        local_frame: Option<u64>,
        network: Option<ClientNetworkQualityReport>,
        runtime_state: ClientRuntimeState,
    ) -> Option<RoomPerformanceSample> {
        let player_index = self.room.player_index_for_connection(connection_id)?;
        let accepted_input_frame = self.room.last_input_frames.get(&player_index).copied();
        let frame_delta = local_frame.map(|frame| {
            let frame = i128::from(frame);
            let canonical_frame = i128::from(self.room.room_frame);

            i64::try_from(frame - canonical_frame).unwrap_or({
                if frame < canonical_frame {
                    i64::MIN
                } else {
                    i64::MAX
                }
            })
        });

        Some(RoomPerformanceSample {
            timestamp_ms: current_timestamp_ms(),
            room_id: self.room.room_id(),
            invite_code: self.room.invite_code().display(),
            event_seq: self.event_seq,
            room_epoch: self.room.room_epoch,
            session_epoch: self.room.session_epoch,
            player_index: player_index.zero_based(),
            runtime_state,
            local_frame,
            canonical_frame: self.room.room_frame,
            released_frame: self.room.released_frame,
            next_release_frame: self.room.next_release_frame,
            accepted_input_frame,
            frame_delta,
            network,
        })
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

    /// Emits an intentional player-exit event.
    pub(super) fn emit_player_exited(&mut self, now: Instant, player_index: u8, reason: String) {
        let room = self.record_event(now, "playerExited", "player intentionally exited");
        let _ = self.events.send(RoomEvent::PlayerExited {
            player_index,
            reason,
            room,
        });
    }

    /// Records that all connected players reported identical state for a frame.
    pub(super) fn record_state_hash_match(&mut self, now: Instant, frame: u64) {
        self.record_event(
            now,
            "stateHashMatched",
            &format!("state hash matched at frame {frame}"),
        );
    }

    /// Records a deterministic state-hash mismatch without forcing recovery.
    pub(super) fn record_state_hash_mismatch_diagnostic(
        &mut self,
        now: Instant,
        mismatch: &StateHashMismatchView,
    ) {
        let detail = state_hash_mismatch_detail("state hash mismatch observed", mismatch);
        self.record_event(now, "stateHashMismatchDiagnostic", &detail);
    }

    /// Records a nearby-frame hash match that should not trigger recovery.
    pub(super) fn record_state_hash_frame_skew_diagnostic(
        &mut self,
        now: Instant,
        mismatch: &StateHashMismatchView,
    ) {
        let detail = state_hash_mismatch_detail("state hash nearby-frame match observed", mismatch);
        self.record_event(now, "stateHashFrameSkewDiagnostic", &detail);
    }

    /// Emits a resync requirement after persistent true state-hash mismatch.
    pub(super) fn emit_state_hash_mismatch(
        &mut self,
        now: Instant,
        mismatch: StateHashMismatchView,
    ) {
        let detail =
            state_hash_mismatch_detail("state hash resync required after mismatch", &mismatch);
        let room = self.record_event(now, "stateHashResyncRequired", &detail);
        let _ = self
            .events
            .send(RoomEvent::StateHashMismatch { mismatch, room });
    }

    /// Emits a scheduled adaptive input-delay update.
    pub(super) fn emit_input_delay_changed(&mut self, now: Instant, change: InputDelayChange) {
        let room = self.record_event(now, "inputDelayChanged", "adaptive input delay scheduled");
        let _ = self
            .events
            .send(RoomEvent::InputDelayChanged { change, room });
    }

    /// Emits a validated snapshot chunk.
    pub(super) fn emit_snapshot_chunk(
        &mut self,
        now: Instant,
        source: ConnectionId,
        chunk: SnapshotChunk,
    ) {
        let room = self.record_event(now, "snapshotChunk", "snapshot chunk relayed");
        let _ = self.events.send(RoomEvent::SnapshotChunk {
            source,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            chunk,
        });
    }

    /// Emits a validated snapshot manifest.
    pub(super) fn emit_snapshot_complete(
        &mut self,
        now: Instant,
        source: ConnectionId,
        manifest: SnapshotManifest,
    ) {
        let room = self.record_event(now, "snapshotComplete", "snapshot manifest relayed");
        let _ = self.events.send(RoomEvent::SnapshotComplete {
            source,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            manifest,
        });
    }

    /// Relays accepted controller input when its server frame is available.
    pub(super) fn relay_accepted_input_frame(&mut self, source: ConnectionId, input: InputFrame) {
        if self
            .room
            .released_frame
            .is_some_and(|released_frame| input.frame <= released_frame)
        {
            self.emit_input_frame_batch(source, input);
            return;
        }

        self.input_relay_buffer
            .push(source, self.room.room_epoch, self.room.session_epoch, input);
    }

    /// Releases one canonical server frame and its ready input batches.
    pub(super) fn emit_next_server_frame(&mut self, _now: Instant) -> Option<ServerFrame> {
        let frame = self.room.release_next_server_frame()?;

        for ready_batch in
            self.input_relay_buffer
                .drain_frame(frame.frame, frame.room_epoch, frame.session_epoch)
        {
            let _ = self.input_events.send(RoomInputEvent::InputFrameBatch {
                batch: ready_batch.batch,
                source: ready_batch.source,
            });
        }

        let _ = self.input_events.send(RoomInputEvent::ServerFrame {
            frame: frame.clone(),
        });

        Some(frame)
    }

    fn emit_input_frame_batch(&self, source: ConnectionId, input: InputFrame) {
        let player_index = input.player_index;
        let batch = InputFrameBatch {
            frames: vec![input],
            player_index,
            room_epoch: self.room.room_epoch,
            session_epoch: self.room.session_epoch,
        };

        let _ = self
            .input_events
            .send(RoomInputEvent::InputFrameBatch { source, batch });
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

fn state_hash_mismatch_detail(prefix: &str, mismatch: &StateHashMismatchView) -> String {
    if mismatch.nearby_matches.is_empty() {
        return format!("{prefix} at frame {}", mismatch.frame);
    }

    let first_match = &mismatch.nearby_matches[0];
    format!(
        "{prefix} at frame {} with {} nearby-frame match(es); first offset {} p{} frame {} matched p{} frame {}",
        mismatch.frame,
        mismatch.nearby_matches.len(),
        first_match.frame_offset,
        first_match.source_player_index.zero_based(),
        first_match.source_frame,
        first_match.matched_player_index.zero_based(),
        first_match.matched_frame
    )
}
