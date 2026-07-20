//! Protocol v5 deterministic state-recovery transaction state.

use crate::protocol::{
    SnapshotManifest, StateHashMismatchView, StateRecoveryPhase, StateRecoveryView,
};
use std::time::{Duration, Instant};

/// Maximum time the host may hold the old epoch while pinning repair state.
pub(crate) const STATE_RECOVERY_PIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Room-owned recovery transaction spanning the old and fresh session epochs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StateRecoveryTransaction {
    recovery_id: u64,
    mismatch: StateHashMismatchView,
    pinned_snapshot: Option<SnapshotManifest>,
    preparing_room_epoch: u64,
    preparing_session_epoch: u64,
    started_at: Instant,
}

impl StateRecoveryTransaction {
    pub(crate) fn preparing(
        recovery_id: u64,
        mismatch: StateHashMismatchView,
        preparing_room_epoch: u64,
        preparing_session_epoch: u64,
        started_at: Instant,
    ) -> Self {
        Self {
            recovery_id,
            mismatch,
            pinned_snapshot: None,
            preparing_room_epoch,
            preparing_session_epoch,
            started_at,
        }
    }

    pub(crate) fn recovery_id(&self) -> u64 {
        self.recovery_id
    }

    pub(crate) fn repair_frame(&self) -> u64 {
        self.mismatch.repair_frame
    }

    pub(crate) fn accepts_message_epoch(&self, room_epoch: u64, session_epoch: u64) -> bool {
        room_epoch == self.preparing_room_epoch && session_epoch == self.preparing_session_epoch
    }

    pub(crate) fn commit(&mut self, manifest: SnapshotManifest) {
        self.pinned_snapshot = Some(manifest);
    }

    pub(crate) fn expected_snapshot(&self) -> Option<&SnapshotManifest> {
        self.pinned_snapshot.as_ref()
    }

    pub(crate) fn is_preparing(&self) -> bool {
        self.pinned_snapshot.is_none()
    }

    pub(crate) fn is_expired(&self, now: Instant) -> bool {
        self.is_preparing()
            && now.saturating_duration_since(self.started_at) >= STATE_RECOVERY_PIN_TIMEOUT
    }

    pub(crate) fn view(&self) -> StateRecoveryView {
        StateRecoveryView {
            recovery_id: self.recovery_id,
            phase: if self.is_preparing() {
                StateRecoveryPhase::Preparing
            } else {
                StateRecoveryPhase::Committed
            },
            repair_frame: self.repair_frame(),
            mismatch: self.mismatch.clone(),
            pinned_snapshot: self.pinned_snapshot.clone(),
        }
    }
}
