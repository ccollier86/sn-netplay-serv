//! V2 scheduled-start state for active rooms.
//!
//! This module owns clock-sample readiness and deterministic-ready transitions.
//! It must not parse WebSocket messages, launch runners, or release frames.

use crate::limits::SCHEDULED_START_MINIMUM_DELAY;
use crate::protocol::{
    ClientNetworkQualityReport, ClockSyncSample, ClockSyncSampleRequest, DeterministicReadyReport,
    ScheduledSessionStart,
};
use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerIndex, PlayerRuntimeState, RoomError, RoomStatus,
};
use std::collections::hash_map::Entry;
use std::time::Instant;

const CLOCK_SYNC_REQUESTED_SAMPLE_COUNT: u8 = 2;
const CLOCK_SYNC_SAFETY_MARGIN_MS: u64 = 100;

/// Result of a v2 start-sync mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartSyncOutcome {
    /// Room is still waiting for more samples or deterministic-ready reports.
    Waiting,
    /// Room scheduled synchronized gameplay release.
    Scheduled(ScheduledSessionStart),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClockSyncSampleRequestState {
    request: ClockSyncSampleRequest,
}

impl ClockSyncSampleRequestState {
    fn new(request: ClockSyncSampleRequest) -> Self {
        Self { request }
    }

    fn request(&self) -> &ClockSyncSampleRequest {
        &self.request
    }
}

impl NetplayRoom {
    /// Creates a lightweight v5 resume schedule after the runtime epoch reset.
    pub(super) fn schedule_v5_resume(
        &mut self,
        start_frame: u64,
        server_time_ms: u64,
    ) -> ScheduledSessionStart {
        let uncertainty_ms = self.scheduled_start_uncertainty_budget();
        let minimum_delay_ms = SCHEDULED_START_MINIMUM_DELAY
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let selected_delay_ms =
            minimum_delay_ms.max(uncertainty_ms.saturating_add(CLOCK_SYNC_SAFETY_MARGIN_MS));
        let start = ScheduledSessionStart {
            room_epoch: self.room_epoch,
            session_epoch: self.session_epoch,
            start_frame,
            server_time_ms: server_time_ms.saturating_add(selected_delay_ms),
            created_at_server_time_ms: server_time_ms,
            minimum_start_delay_ms: minimum_delay_ms,
            clock_uncertainty_budget_ms: uncertainty_ms,
        };
        self.scheduled_start = Some(start.clone());
        start
    }

    /// Returns whether all connected players opted into scheduled start.
    pub(super) fn connected_players_support_scheduled_start(&self) -> bool {
        if !self.is_controller_netplay() {
            return false;
        }

        let connected = self.connected_player_indices();
        connected.len() == usize::from(self.max_players)
            && connected.iter().all(|player_index| {
                self.slot_for_player(*player_index)
                    .is_some_and(|slot| slot.supports_scheduled_start && slot.supports_clock_sync)
            })
    }

    /// Returns whether all connected v2 players are ready for clock sampling.
    pub(super) fn should_request_clock_sync_sample(&self) -> bool {
        self.status == RoomStatus::Ready
            && self.connected_players_support_scheduled_start()
            && self.connected_players_are_ready_for_start_sync()
            && !self.connected_players_have_clock_samples()
            && self.scheduled_start.is_none()
    }

    /// Builds or returns the active clock-sample request for this room.
    pub(super) fn request_clock_sync_sample(
        &mut self,
        server_time_ms: u64,
    ) -> ClockSyncSampleRequest {
        if let Some(active) = self.clock_sync_request.as_ref() {
            return active.request().clone();
        }

        let request = ClockSyncSampleRequest {
            request_id: format!(
                "clock-{}-{}-{}",
                self.room_epoch, self.session_epoch, self.next_clock_sync_request_id
            ),
            requested_sample_count: CLOCK_SYNC_REQUESTED_SAMPLE_COUNT,
            server_send_time_ms: server_time_ms,
        };
        self.next_clock_sync_request_id = self.next_clock_sync_request_id.saturating_add(1);
        self.clock_sync_request = Some(ClockSyncSampleRequestState::new(request.clone()));
        request
    }

    /// Records one clock sample and schedules start if all peers are ready.
    pub(super) fn accept_clock_sync_sample(
        &mut self,
        connection_id: ConnectionId,
        sample: ClockSyncSample,
        now: Instant,
        server_receive_time_ms: u64,
    ) -> Result<StartSyncOutcome, RoomError> {
        if !self.connected_players_support_scheduled_start() {
            return Err(RoomError::RoomNotReady);
        }

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;
        let request = self
            .clock_sync_request
            .as_ref()
            .ok_or(RoomError::RoomNotReady)?
            .request();

        if sample.request_id != request.request_id
            || sample.sample_index >= request.requested_sample_count
            || sample.server_send_time_ms != request.server_send_time_ms
            || sample.client_send_time_ms < sample.client_receive_time_ms
            || server_receive_time_ms < sample.server_send_time_ms
        {
            return Err(RoomError::InvalidPayload);
        }

        let server_elapsed = server_receive_time_ms - sample.server_send_time_ms;
        let client_elapsed = sample.client_send_time_ms - sample.client_receive_time_ms;
        let round_trip_ms = server_elapsed.saturating_sub(client_elapsed);
        let uncertainty_ms = round_trip_ms / 2;

        match self.clock_uncertainty_by_player.entry(player_index) {
            Entry::Occupied(mut entry) => {
                if uncertainty_ms < *entry.get() {
                    entry.insert(uncertainty_ms);
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(uncertainty_ms);
            }
        }
        self.clock_sample_indices_by_player
            .entry(player_index)
            .or_default()
            .insert(sample.sample_index);

        Ok(self.try_schedule_start(now, server_receive_time_ms))
    }

    /// Marks one player deterministic-ready and schedules start if possible.
    pub(super) fn mark_deterministic_ready(
        &mut self,
        connection_id: ConnectionId,
        _report: DeterministicReadyReport,
        network: Option<ClientNetworkQualityReport>,
        now: Instant,
        server_time_ms: u64,
    ) -> Result<StartSyncOutcome, RoomError> {
        if self.status != RoomStatus::Ready && self.status != RoomStatus::StartScheduled {
            return Err(RoomError::RoomNotReady);
        }

        if !self.connected_players_support_scheduled_start() {
            return Err(RoomError::RoomNotReady);
        }

        let player_index = self
            .player_index_for_connection(connection_id)
            .ok_or(RoomError::UnknownConnection)?;

        self.record_network_report(connection_id, None, network, now);
        self.deterministic_ready_players.insert(player_index);
        if let Some(slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index)
        {
            slot.runtime_state = PlayerRuntimeState::DeterministicReady;
        }

        Ok(self.try_schedule_start(now, server_time_ms))
    }

    /// Clears all v2 start-sync state for a new room/session epoch.
    pub(super) fn reset_start_sync_state(&mut self) {
        self.clock_sync_request = None;
        self.clock_uncertainty_by_player.clear();
        self.clock_sample_indices_by_player.clear();
        self.deterministic_ready_players.clear();
        self.scheduled_start = None;
    }

    pub(super) fn scheduled_start(&self) -> Option<&ScheduledSessionStart> {
        self.scheduled_start.as_ref()
    }

    fn try_schedule_start(&mut self, now: Instant, server_time_ms: u64) -> StartSyncOutcome {
        if let Some(start) = self.scheduled_start.as_ref() {
            return StartSyncOutcome::Scheduled(start.clone());
        }

        if !self.connected_players_ready_for_scheduling() {
            return StartSyncOutcome::Waiting;
        }

        self.apply_initial_v5_input_delay(now);

        let uncertainty_ms = self.scheduled_start_uncertainty_budget();
        let selected_delay_ms = SCHEDULED_START_MINIMUM_DELAY
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
            .max(uncertainty_ms.saturating_add(CLOCK_SYNC_SAFETY_MARGIN_MS));
        let start = ScheduledSessionStart {
            room_epoch: self.room_epoch,
            session_epoch: self.session_epoch,
            start_frame: self.sync_start_frame,
            server_time_ms: server_time_ms.saturating_add(selected_delay_ms),
            created_at_server_time_ms: server_time_ms,
            minimum_start_delay_ms: SCHEDULED_START_MINIMUM_DELAY
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            clock_uncertainty_budget_ms: uncertainty_ms,
        };

        self.status = RoomStatus::StartScheduled;
        self.scheduled_start = Some(start.clone());
        StartSyncOutcome::Scheduled(start)
    }

    fn connected_players_ready_for_scheduling(&self) -> bool {
        let connected = self.connected_player_indices();
        connected.len() == usize::from(self.max_players)
            && connected
                .iter()
                .all(|player_index| self.deterministic_ready_players.contains(player_index))
            && self.connected_players_have_clock_samples()
    }

    fn connected_players_have_clock_samples(&self) -> bool {
        let Some(request) = self
            .clock_sync_request
            .as_ref()
            .map(ClockSyncSampleRequestState::request)
        else {
            return false;
        };
        let required_count = usize::from(request.requested_sample_count);

        self.connected_player_indices().iter().all(|player_index| {
            self.clock_sample_indices_by_player
                .get(player_index)
                .is_some_and(|samples| samples.len() >= required_count)
        })
    }

    fn connected_players_are_ready_for_start_sync(&self) -> bool {
        self.connected_player_indices()
            .iter()
            .all(|player_index| self.ready_players.contains(player_index))
    }

    fn scheduled_start_uncertainty_budget(&self) -> u64 {
        let clock_budget = self
            .clock_uncertainty_by_player
            .values()
            .copied()
            .max()
            .unwrap_or(0);
        let network_budget = self
            .players
            .iter()
            .filter_map(|slot| slot.latest_network_report.as_ref())
            .flat_map(|report| {
                [
                    report.clock_uncertainty_ms.map(u64::from),
                    report.jitter_ms.map(u64::from),
                    report.round_trip_ms.map(|value| u64::from(value) / 2),
                ]
            })
            .flatten()
            .max()
            .unwrap_or(0);

        clock_budget.max(network_budget)
    }

    fn slot_for_player(&self, player_index: PlayerIndex) -> Option<&crate::rooms::PlayerSlot> {
        self.players
            .iter()
            .find(|slot| slot.player_index == player_index)
    }
}
