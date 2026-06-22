//! V2 scheduled-start registry operations.
//!
//! This module is the synchronization boundary for clock samples and
//! deterministic-ready reports. It emits room events but does not parse
//! transport messages or release gameplay frames.

use super::InMemoryRoomRegistry;
use crate::protocol::{ClientNetworkQualityReport, ClockSyncSample, DeterministicReadyReport};
use crate::rooms::{ConnectionId, InviteCode, RoomError, RoomView, StartSyncOutcome};

impl InMemoryRoomRegistry {
    /// Records one client clock sample for a scheduled-start room.
    pub(super) async fn record_clock_sync_sample_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        sample: ClockSyncSample,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let server_time_ms = self.server_time_ms_at(now);
        let outcome =
            stored_room
                .room
                .accept_clock_sync_sample(connection_id, sample, server_time_ms)?;

        match outcome {
            StartSyncOutcome::Waiting => {
                stored_room.emit_state(now, "clockSyncSampleAccepted", "clock sample accepted");
            }
            StartSyncOutcome::Scheduled(start) => {
                stored_room.emit_scheduled_start(now, start);
            }
        }

        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));
        Ok(room)
    }

    /// Marks a connected client ready for synchronized frame release.
    pub(super) async fn mark_deterministic_ready_impl(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        report: DeterministicReadyReport,
        network: Option<ClientNetworkQualityReport>,
    ) -> Result<RoomView, RoomError> {
        let mut rooms = self.invite_codes.write().await;
        let stored_room = rooms
            .get_mut(invite_code.normalized())
            .ok_or(RoomError::NotFound)?;
        let now = self.clock.now();
        let server_time_ms = self.server_time_ms_at(now);
        let outcome = stored_room.room.mark_deterministic_ready(
            connection_id,
            report,
            network,
            now,
            server_time_ms,
        )?;

        match outcome {
            StartSyncOutcome::Waiting => {
                stored_room.emit_state(
                    now,
                    "playerDeterministicReady",
                    "player deterministic ready",
                );
            }
            StartSyncOutcome::Scheduled(start) => {
                stored_room.emit_scheduled_start(now, start);
            }
        }

        let room = stored_room.view(now);
        self.record_recent_events(stored_room.debug_events(1));
        Ok(room)
    }
}
