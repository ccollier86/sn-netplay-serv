//! Protocol v5 deterministic state-recovery payloads.
//!
//! Recovery is deliberately two-phase. Clients first freeze the old session
//! epoch while the host pins an exact start-of-frame snapshot. Only after the
//! relay accepts that manifest does it commit a fresh session epoch.

use crate::protocol::{SnapshotManifest, StateHashMismatchView};
use serde::{Deserialize, Serialize};

/// Current phase of one protocol v5 state-recovery transaction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StateRecoveryPhase {
    /// The old epoch is frozen while the host prepares the repair snapshot.
    Preparing,
    /// A pinned snapshot was accepted and a fresh session epoch was created.
    Committed,
}

/// Serializable state for one deterministic repair transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateRecoveryView {
    /// Monotonic room-local transaction id.
    pub recovery_id: u64,
    /// Current transaction phase.
    pub phase: StateRecoveryPhase,
    /// Exact start-of-frame state every client must restore.
    pub repair_frame: u64,
    /// Hash comparison that initiated the transaction.
    pub mismatch: StateHashMismatchView,
    /// Exact host snapshot accepted before the new epoch was committed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_snapshot: Option<SnapshotManifest>,
}

/// Host acknowledgement that the exact repair state is durable locally.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateRecoveryPin {
    /// Transaction id supplied by the prepare event.
    pub recovery_id: u64,
    /// Manifest for the already-created snapshot that will be transferred.
    pub manifest: SnapshotManifest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ClientMessage, PlayerStateHashView};
    use crate::rooms::PlayerIndex;

    #[test]
    fn canonical_recovery_json_fixtures_match_the_wire_contract() {
        let preparing = recovery(StateRecoveryPhase::Preparing, None);
        assert_fixture(
            include_str!("../../spec/netplay-v5/fixtures/state-recovery-prepare.json"),
            serde_json::to_value(preparing).expect("serialize prepare"),
        );

        let pinned: ClientMessage = serde_json::from_str(include_str!(
            "../../spec/netplay-v5/fixtures/state-recovery-pinned.json"
        ))
        .expect("decode pinned fixture");
        assert_eq!(
            pinned,
            ClientMessage::StateRecoveryPinned {
                room_epoch: 3,
                session_epoch: 8,
                pin: StateRecoveryPin {
                    recovery_id: 7,
                    manifest: manifest(),
                },
            }
        );

        let committed = recovery(StateRecoveryPhase::Committed, Some(manifest()));
        assert_fixture(
            include_str!("../../spec/netplay-v5/fixtures/state-recovery-committed.json"),
            serde_json::to_value(committed).expect("serialize committed"),
        );
    }

    fn recovery(
        phase: StateRecoveryPhase,
        pinned_snapshot: Option<SnapshotManifest>,
    ) -> StateRecoveryView {
        StateRecoveryView {
            recovery_id: 7,
            phase,
            repair_frame: 120,
            mismatch: StateHashMismatchView {
                frame: 120,
                repair_frame: 120,
                hashes: vec![
                    PlayerStateHashView {
                        player_index: PlayerIndex::ONE,
                        sha256: "a".repeat(64),
                    },
                    PlayerStateHashView {
                        player_index: PlayerIndex::TWO,
                        sha256: "b".repeat(64),
                    },
                ],
                nearby_matches: Vec::new(),
            },
            pinned_snapshot,
        }
    }

    fn manifest() -> SnapshotManifest {
        SnapshotManifest {
            snapshot_id: "recovery-7".to_string(),
            repair_frame: 120,
            total_bytes: 4,
            sha256: "c".repeat(64),
        }
    }

    fn assert_fixture(fixture: &str, actual: serde_json::Value) {
        let expected = serde_json::from_str::<serde_json::Value>(fixture).expect("fixture JSON");
        assert_eq!(actual, expected);
    }
}
