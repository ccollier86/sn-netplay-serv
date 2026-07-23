//! Stored room wrapper with event broadcasting.
//!
//! The registry owns lookup and locking; this wrapper owns the event channel
//! beside a room and exposes small emit helpers. It does not parse protocol
//! messages or apply room-domain rules.

use crate::protocol::{
    ClientNetworkQualityReport, ClientRuntimeState, ClockSyncSampleRequest, FastInputFrame,
    InputDelayChange, InputFrame, InputFrameBatch, RomRelayCancelled, RomRelayCompletion,
    RomRelayFailure, RomRelayGrant, RomRelayProgress, ScheduledSessionStart, ServerFrame,
    ServerFrameReleaseV5, SessionPauseView, SnapshotChunk, SnapshotFileRelayGrant,
    SnapshotManifest, StateHashMismatchView, StateRecoveryView, StrictInputBatch,
};
use crate::rooms::{
    ConnectionId, FastInputRelayBuffer, InputFrameRelayBuffer, NetplayRoom, RoomDebugEvent,
    RoomDebugEventLog, RoomEvent, RoomInputEvent, RoomPerformanceSample, RoomView,
    current_timestamp_ms,
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
    fast_input_relay_buffer: FastInputRelayBuffer,
    relay_room_epoch: u64,
    relay_session_epoch: u64,
    event_seq: u64,
    debug_events: RoomDebugEventLog,
    created_at: Instant,
}

impl StoredRoom {
    /// Creates a stored room with a bounded event channel.
    pub(super) fn new(room: NetplayRoom, now: Instant) -> Self {
        let (events, _) = broadcast::channel(ROOM_EVENT_CHANNEL_CAPACITY);
        let (input_events, _) = broadcast::channel(INPUT_EVENT_CHANNEL_CAPACITY);

        let relay_room_epoch = room.room_epoch;
        let relay_session_epoch = room.session_epoch;
        Self {
            room,
            events,
            input_events,
            input_relay_buffer: InputFrameRelayBuffer::default(),
            fast_input_relay_buffer: FastInputRelayBuffer::default(),
            relay_room_epoch,
            relay_session_epoch,
            event_seq: 0,
            debug_events: RoomDebugEventLog::default(),
            created_at: now,
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

    /// Closes and reports a v5 recovery whose host never pinned exact state.
    pub(super) fn expire_state_recovery(&mut self, now: Instant) -> bool {
        let Some(recovery) = self.room.state_recovery_view() else {
            return false;
        };
        if !self.room.close_expired_state_recovery(now) {
            return false;
        }
        self.emit_state_recovery_failed(now, recovery, "snapshotPinTimedOut");
        true
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
            protocol_version: self.room.protocol_version(),
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
    pub(super) fn emit_state(&mut self, now: Instant, kind: &str, detail: &str) {
        let room = self.record_event(now, kind, detail);
        let _ = self.events.send(RoomEvent::RoomStateChanged(room));
    }

    /// Records a debug-only event without broadcasting a room-state update.
    pub(super) fn record_debug_event(&mut self, now: Instant, kind: &str, detail: &str) {
        self.record_event(now, kind, detail);
    }

    /// Records a diagnostic observation without advancing the public event
    /// sequence or broadcasting a room-state update.
    pub(super) fn record_diagnostic_observation(&mut self, now: Instant, kind: &str, detail: &str) {
        let room = self.room.view_for_event(self.event_seq, now);
        self.push_debug_event(&room, kind, detail);
    }

    /// Emits a session-start event.
    pub(super) fn emit_start(&mut self, now: Instant, start_frame: u64) {
        let room = self.record_event(now, "sessionStarted", "session started");
        let _ = self.events.send(RoomEvent::SessionStarted {
            start_frame,
            scheduled_start: None,
            room,
        });
    }

    /// Emits a future scheduled session-start event.
    pub(super) fn emit_scheduled_start(&mut self, now: Instant, start: ScheduledSessionStart) {
        let room = self.record_event(now, "sessionStartScheduled", "session start scheduled");
        let _ = self.events.send(RoomEvent::SessionStarted {
            start_frame: start.start_frame,
            scheduled_start: Some(start),
            room,
        });
    }

    /// Emits a v2 startup clock-sample request.
    pub(super) fn emit_clock_sync_sample_requested(
        &mut self,
        now: Instant,
        request: ClockSyncSampleRequest,
    ) {
        let room = self.record_event(now, "clockSyncSampleRequested", "clock sample requested");
        let _ = self.events.send(RoomEvent::ClockSyncSampleRequested {
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            request,
        });
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
        scheduled_start: Option<ScheduledSessionStart>,
    ) {
        self.synchronize_input_runtime_epoch();
        let room = self.record_event(now, "resumeScheduled", "coordinated resume scheduled");
        let _ = self.events.send(RoomEvent::SessionResumeScheduled {
            sequence,
            resume_at_frame,
            scheduled_start,
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

    /// Records nearby-frame hash matches for diagnostics.
    pub(super) fn record_state_hash_frame_skew_diagnostic(
        &mut self,
        now: Instant,
        mismatch: &StateHashMismatchView,
    ) {
        let detail = state_hash_mismatch_detail("state hash nearby-frame match observed", mismatch);
        self.record_event(now, "stateHashFrameSkewDiagnostic", &detail);
    }

    /// Emits a resync requirement after exact-frame state-hash mismatch.
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

    /// Emits the old-epoch protocol v5 repair freeze.
    pub(super) fn emit_state_recovery_prepare(
        &mut self,
        now: Instant,
        recovery: StateRecoveryView,
    ) {
        let detail = format!(
            "state recovery {} preparing at frame {}",
            recovery.recovery_id, recovery.repair_frame
        );
        let room = self.record_event(now, "stateRecoveryPrepare", &detail);
        let _ = self
            .events
            .send(RoomEvent::StateRecoveryPrepare { recovery, room });
    }

    /// Emits the fresh-epoch protocol v5 repair commit.
    pub(super) fn emit_state_recovery_committed(
        &mut self,
        now: Instant,
        recovery: StateRecoveryView,
    ) {
        self.synchronize_input_runtime_epoch();
        let detail = format!(
            "state recovery {} committed at frame {}",
            recovery.recovery_id, recovery.repair_frame
        );
        let room = self.record_event(now, "stateRecoveryCommitted", &detail);
        let _ = self
            .events
            .send(RoomEvent::StateRecoveryCommitted { recovery, room });
    }

    /// Emits a bounded state-recovery failure before the room is removed.
    pub(super) fn emit_state_recovery_failed(
        &mut self,
        now: Instant,
        recovery: StateRecoveryView,
        reason: &str,
    ) {
        let detail = format!("state recovery {} failed: {}", recovery.recovery_id, reason);
        let room = self.record_event(now, "stateRecoveryFailed", &detail);
        let _ = self.events.send(RoomEvent::StateRecoveryFailed {
            recovery,
            reason: reason.to_string(),
            room,
        });
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

    /// Emits a private file-relay upload grant to the host.
    pub(super) fn emit_snapshot_file_relay_upload_granted(
        &mut self,
        now: Instant,
        source: ConnectionId,
        grant: SnapshotFileRelayGrant,
    ) {
        let room = self.record_event(
            now,
            "snapshotFileRelayUploadGranted",
            "snapshot file relay upload granted",
        );
        let _ = self.events.send(RoomEvent::SnapshotFileRelayUploadGranted {
            source,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            grant,
        });
    }

    /// Emits a private file-relay download grant to the guest.
    pub(super) fn emit_snapshot_file_relay_download_ready(
        &mut self,
        now: Instant,
        receiver: ConnectionId,
        grant: SnapshotFileRelayGrant,
    ) {
        let room = self.record_event(
            now,
            "snapshotFileRelayDownloadReady",
            "snapshot file relay download ready",
        );
        let _ = self.events.send(RoomEvent::SnapshotFileRelayDownloadReady {
            receiver,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            grant,
        });
    }

    /// Emits a private ROM file-relay upload grant to the host.
    pub(super) fn emit_rom_relay_upload_granted(&mut self, now: Instant, grant: RomRelayGrant) {
        let source = self
            .room
            .rom_relay_transfer
            .as_ref()
            .map(|transfer| transfer.sender_connection)
            .expect("rom relay transfer exists before upload grant");
        let room = self.record_event(now, "romRelayUploadGranted", "ROM relay upload granted");
        let _ = self.events.send(RoomEvent::RomRelayUploadGranted {
            source,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            grant,
        });
    }

    /// Emits a private ROM file-relay download grant to the guest.
    pub(super) fn emit_rom_relay_download_granted(&mut self, now: Instant, grant: RomRelayGrant) {
        let receiver = self
            .room
            .rom_relay_transfer
            .as_ref()
            .map(|transfer| transfer.receiver_connection)
            .expect("rom relay transfer exists before download grant");
        let room = self.record_event(now, "romRelayDownloadGranted", "ROM relay download granted");
        let _ = self.events.send(RoomEvent::RomRelayDownloadGranted {
            receiver,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            grant,
        });
    }

    /// Emits a ROM relay progress event.
    pub(super) fn emit_rom_relay_progress(
        &mut self,
        now: Instant,
        source: ConnectionId,
        progress: RomRelayProgress,
    ) {
        let room = self.record_event(now, "romRelayProgress", "ROM relay progress");
        let _ = self.events.send(RoomEvent::RomRelayProgress {
            source,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            progress,
        });
    }

    /// Emits a ROM relay completion event.
    pub(super) fn emit_rom_relay_completed(
        &mut self,
        now: Instant,
        source: ConnectionId,
        completion: RomRelayCompletion,
    ) {
        let room = self.record_event(now, "romRelayCompleted", "ROM relay completed");
        let _ = self.events.send(RoomEvent::RomRelayCompleted {
            source,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            completion,
        });
    }

    /// Emits a ROM relay failure event.
    pub(super) fn emit_rom_relay_failed(
        &mut self,
        now: Instant,
        source: ConnectionId,
        failure: RomRelayFailure,
    ) {
        let room = self.record_event(now, "romRelayFailed", "ROM relay failed");
        let _ = self.events.send(RoomEvent::RomRelayFailed {
            source,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            failure,
        });
    }

    /// Emits a ROM relay cancellation event.
    pub(super) fn emit_rom_relay_cancelled(
        &mut self,
        now: Instant,
        source: ConnectionId,
        cancelled: RomRelayCancelled,
    ) {
        let room = self.record_event(now, "romRelayCancelled", "ROM relay cancelled");
        let _ = self.events.send(RoomEvent::RomRelayCancelled {
            source,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            cancelled,
        });
    }

    /// Relays accepted controller input when its server frame is available.
    pub(super) fn relay_accepted_input_frame(&mut self, source: ConnectionId, input: InputFrame) {
        self.synchronize_input_runtime_epoch();
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

    /// Relays an accepted fast-input record when its server frame is available.
    pub(super) fn relay_accepted_fast_input_frame(
        &mut self,
        source: ConnectionId,
        input: FastInputFrame,
    ) {
        self.synchronize_input_runtime_epoch();
        if self
            .room
            .released_frame
            .is_some_and(|released_frame| input.frame <= released_frame)
        {
            self.emit_fast_input_frame(source, input);
            return;
        }

        self.fast_input_relay_buffer.push(source, input);
    }

    /// Releases one canonical server frame and its ready input batches.
    pub(super) fn emit_next_server_frame(
        &mut self,
        _now: Instant,
        server_time_ms: u64,
    ) -> Option<ServerFrame> {
        self.synchronize_input_runtime_epoch();
        let frame = self.room.release_next_server_frame(server_time_ms)?;

        for ready_batch in
            self.input_relay_buffer
                .drain_frame(frame.frame, frame.room_epoch, frame.session_epoch)
        {
            let _ = self.input_events.send(RoomInputEvent::InputFrameBatch {
                batch: ready_batch.batch,
                source: ready_batch.source,
            });
        }

        for ready_frame in self.fast_input_relay_buffer.drain_frame(
            frame.frame,
            frame.room_epoch,
            frame.session_epoch,
        ) {
            let _ = self.input_events.send(RoomInputEvent::FastInputFrame {
                source: ready_frame.source,
                frame: ready_frame.frame,
            });
        }

        let _ = self.input_events.send(RoomInputEvent::ServerFrame {
            frame: frame.clone(),
        });

        Some(frame)
    }

    /// Immediately publishes a newly accepted strict-input suffix to its peer.
    pub(super) fn emit_strict_input_batch(
        &mut self,
        source: ConnectionId,
        batch: StrictInputBatch,
    ) {
        self.synchronize_input_runtime_epoch();
        let _ = self
            .input_events
            .send(RoomInputEvent::StrictInputBatch { source, batch });
    }

    /// Publishes one host-driven v5 frame release to every input socket.
    pub(super) fn emit_v5_server_frame(&mut self, release: ServerFrameReleaseV5) {
        self.synchronize_input_runtime_epoch();
        let _ = self
            .input_events
            .send(RoomInputEvent::ServerFrameV5 { release });
    }

    /// Releases an early first host open when its scheduled deadline is due.
    pub(super) fn emit_due_v5_server_frame(&mut self, server_time_ms: u64) -> bool {
        self.synchronize_input_runtime_epoch();
        let Some(release) = self.room.release_due_v5_host_frame(server_time_ms) else {
            return false;
        };
        self.emit_v5_server_frame(release);
        true
    }

    fn emit_fast_input_frame(&self, source: ConnectionId, frame: FastInputFrame) {
        let _ = self
            .input_events
            .send(RoomInputEvent::FastInputFrame { source, frame });
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

    fn synchronize_input_runtime_epoch(&mut self) {
        if self.relay_room_epoch == self.room.room_epoch
            && self.relay_session_epoch == self.room.session_epoch
        {
            return;
        }

        self.input_relay_buffer = InputFrameRelayBuffer::default();
        self.fast_input_relay_buffer = FastInputRelayBuffer::default();
        self.relay_room_epoch = self.room.room_epoch;
        self.relay_session_epoch = self.room.session_epoch;
    }

    fn record_event(&mut self, now: Instant, kind: &str, detail: &str) -> RoomView {
        self.event_seq = self.event_seq.saturating_add(1);
        let room = self.room.view_for_event(self.event_seq, now);
        self.push_debug_event(&room, kind, detail);
        room
    }

    fn push_debug_event(&mut self, room: &RoomView, kind: &str, detail: &str) {
        self.debug_events.push(RoomDebugEvent {
            timestamp_ms: current_timestamp_ms(),
            room_id: room.room_id,
            invite_code: room.invite_code.clone(),
            event_seq: room.event_seq,
            room_epoch: room.room_epoch,
            session_epoch: room.session_epoch,
            protocol_version: self.room.protocol_version(),
            kind: kind.to_string(),
            detail: detail.to_string(),
        });
    }
}

fn state_hash_mismatch_detail(prefix: &str, mismatch: &StateHashMismatchView) -> String {
    if mismatch.nearby_matches.is_empty() {
        return format!(
            "{prefix} at frame {}; repair frame {}",
            mismatch.frame, mismatch.repair_frame
        );
    }

    let first_match = &mismatch.nearby_matches[0];
    format!(
        "{prefix} at frame {} with repair frame {} and {} nearby-frame match(es); first offset {} p{} frame {} matched p{} frame {}",
        mismatch.frame,
        mismatch.repair_frame,
        mismatch.nearby_matches.len(),
        first_match.frame_offset,
        first_match.source_player_index.zero_based(),
        first_match.source_frame,
        first_match.matched_player_index.zero_based(),
        first_match.matched_frame
    )
}
