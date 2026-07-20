//! Protocol-v5 lightweight pause/resume epoch transitions.

use crate::protocol::ScheduledSessionStart;
use crate::rooms::{NetplayRoom, PlayerRuntimeState, PlayerStatus, RoomStatus};

impl NetplayRoom {
    /// Clears volatile deterministic state and schedules resume without a state transfer.
    pub(super) fn begin_v5_pause_resume(
        &mut self,
        resume_frame: u64,
        server_time_ms: u64,
    ) -> ScheduledSessionStart {
        self.bump_session_epoch();
        self.last_input_frames.clear();
        self.next_input_frames.clear();
        self.pause_state = None;
        self.snapshot_transfer = None;
        self.snapshot_file_relay_transfer = None;
        self.sync_start_frame = resume_frame;
        self.room_frame = resume_frame;
        self.released_frame = None;
        self.next_release_frame = resume_frame;
        self.pending_input_delay_change = None;
        self.state_hashes.clear();
        self.state_hash_true_mismatch_streak = 0;
        self.reset_authoritative_state_hash_cursor_for_resume(resume_frame);
        self.pending_host_frame_open = None;
        self.scheduled_start = None;
        self.status = RoomStatus::StartScheduled;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Ready;
                slot.runtime_state = PlayerRuntimeState::DeterministicReady;
            });
        self.schedule_v5_resume(resume_frame, server_time_ms)
    }
}
