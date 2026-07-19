//! Two-phase deterministic state repair for protocol v5 rooms.

use crate::protocol::{
    SnapshotChunk, SnapshotLimits, SnapshotManifest, StateHashMismatchView, StateRecoveryPin,
    StateRecoveryView,
};
use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerRole, PlayerRuntimeState, PlayerStatus, RoomError, RoomStatus,
    StateRecoveryTransaction,
};
use std::time::{Duration, Instant};

const STATE_RECOVERY_ATTEMPT_WINDOW: Duration = Duration::from_secs(60);
const STATE_RECOVERY_MAX_ATTEMPTS: usize = 2;

/// Result of starting an authoritative protocol v5 repair transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StateRecoveryStartOutcome {
    /// The old epoch is frozen while the host pins exact state.
    Preparing(StateRecoveryView),
    /// Repeated divergence exceeded the bounded repair policy and closed the room.
    AttemptLimitExceeded(StateRecoveryView),
}

impl NetplayRoom {
    /// Freezes the old epoch so the host can pin exact start-of-frame state.
    pub(super) fn begin_v5_state_recovery(
        &mut self,
        mismatch: StateHashMismatchView,
        now: Instant,
    ) -> Result<StateRecoveryStartOutcome, RoomError> {
        if !self.uses_strict_controller_input() || self.state_recovery.is_some() {
            return Err(RoomError::RoomNotReady);
        }

        self.state_recovery_started_at.retain(|started_at| {
            now.saturating_duration_since(*started_at) < STATE_RECOVERY_ATTEMPT_WINDOW
        });

        let recovery_id = self.next_state_recovery_id;
        self.next_state_recovery_id = self.next_state_recovery_id.saturating_add(1);
        self.state_recovery = Some(StateRecoveryTransaction::preparing(
            recovery_id,
            mismatch,
            now,
        ));

        if self.state_recovery_started_at.len() >= STATE_RECOVERY_MAX_ATTEMPTS {
            let recovery = self
                .state_recovery
                .as_ref()
                .map(StateRecoveryTransaction::view)
                .ok_or(RoomError::RoomNotReady)?;
            self.close_state_recovery_room();
            return Ok(StateRecoveryStartOutcome::AttemptLimitExceeded(recovery));
        }

        self.state_recovery_started_at.push_back(now);
        self.status = RoomStatus::RepairingState;
        self.pending_host_frame_open = None;
        self.scheduled_start = None;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Paused;
                slot.runtime_state = PlayerRuntimeState::Pausing;
            });

        let recovery = self
            .state_recovery
            .as_ref()
            .map(StateRecoveryTransaction::view)
            .ok_or(RoomError::RoomNotReady)?;
        Ok(StateRecoveryStartOutcome::Preparing(recovery))
    }

    /// Accepts the host's durable snapshot identity and commits a fresh epoch.
    pub(super) fn accept_v5_state_recovery_pin(
        &mut self,
        connection_id: ConnectionId,
        pin: StateRecoveryPin,
        limits: SnapshotLimits,
    ) -> Result<StateRecoveryView, RoomError> {
        if !self.uses_strict_controller_input() || self.status != RoomStatus::RepairingState {
            return Err(RoomError::RoomNotReady);
        }

        match self
            .players
            .iter()
            .find(|slot| slot.connection_id == Some(connection_id))
            .map(|slot| slot.role)
        {
            Some(PlayerRole::Host) => {}
            Some(PlayerRole::Guest) => return Err(RoomError::HostOnly),
            None => return Err(RoomError::UnknownConnection),
        }

        pin.manifest
            .validate(limits)
            .map_err(|_| RoomError::SnapshotInvalid)?;
        let transaction = self
            .state_recovery
            .as_ref()
            .ok_or(RoomError::RoomNotReady)?;
        if !transaction.is_preparing()
            || transaction.recovery_id() != pin.recovery_id
            || transaction.repair_frame() != pin.manifest.repair_frame
        {
            return Err(RoomError::SnapshotInvalid);
        }

        let repair_frame = transaction.repair_frame();
        let mut transaction = self.state_recovery.take().ok_or(RoomError::RoomNotReady)?;
        transaction.commit(pin.manifest);
        self.reset_sync_state_to(repair_frame);
        self.state_recovery = Some(transaction);
        self.bump_session_epoch();
        self.status = RoomStatus::CheckingCompatibility;
        self.players
            .iter_mut()
            .filter(|slot| slot.connection_id.is_some())
            .for_each(|slot| {
                slot.status = PlayerStatus::Connected;
                slot.runtime_state = PlayerRuntimeState::Connected;
            });

        self.state_recovery
            .as_ref()
            .map(StateRecoveryTransaction::view)
            .ok_or(RoomError::RoomNotReady)
    }

    /// Ensures recovery transfers use the exact pre-commit host snapshot.
    pub(super) fn validate_v5_recovery_snapshot(
        &self,
        manifest: &SnapshotManifest,
    ) -> Result<(), RoomError> {
        let Some(expected) = self
            .state_recovery
            .as_ref()
            .and_then(StateRecoveryTransaction::expected_snapshot)
        else {
            return Ok(());
        };

        if expected == manifest {
            Ok(())
        } else {
            Err(RoomError::SnapshotInvalid)
        }
    }

    /// Rejects recovery chunks that do not belong to the pinned snapshot.
    pub(super) fn validate_v5_recovery_snapshot_chunk(
        &self,
        chunk: &SnapshotChunk,
    ) -> Result<(), RoomError> {
        let Some(expected) = self
            .state_recovery
            .as_ref()
            .and_then(StateRecoveryTransaction::expected_snapshot)
        else {
            return Ok(());
        };

        if expected.snapshot_id == chunk.snapshot_id && expected.repair_frame == chunk.repair_frame
        {
            Ok(())
        } else {
            Err(RoomError::SnapshotInvalid)
        }
    }

    /// Returns whether a recovery prepare transaction exceeded its pin window.
    pub(super) fn state_recovery_pin_expired(&self, now: Instant) -> bool {
        self.state_recovery
            .as_ref()
            .is_some_and(|transaction| transaction.is_expired(now))
    }

    /// Returns the active recovery transaction for event delivery.
    pub(super) fn state_recovery_view(&self) -> Option<StateRecoveryView> {
        self.state_recovery
            .as_ref()
            .map(StateRecoveryTransaction::view)
    }

    /// Closes a room whose host did not pin repair state in time.
    pub(super) fn close_expired_state_recovery(&mut self, now: Instant) -> bool {
        if !self.state_recovery_pin_expired(now) {
            return false;
        }

        self.close_state_recovery_room();
        true
    }

    /// Clears the completed transaction once deterministic gameplay resumes.
    pub(super) fn finish_v5_state_recovery(&mut self) {
        self.state_recovery = None;
    }

    fn close_state_recovery_room(&mut self) {
        self.status = RoomStatus::Closed;
        self.pending_host_frame_open = None;
        self.scheduled_start = None;
        self.players
            .iter_mut()
            .filter(|slot| !slot.is_empty())
            .for_each(|slot| {
                slot.status = PlayerStatus::Disconnected;
                slot.runtime_state = PlayerRuntimeState::Disconnected;
            });
    }
}
