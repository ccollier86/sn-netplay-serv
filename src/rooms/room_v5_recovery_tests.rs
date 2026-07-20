use crate::protocol::{
    PlayerStateHashView, SnapshotLimits, SnapshotManifest, StateDigestMode, StateHashMismatchView,
    StateHashReport, StateRecoveryPhase, StateRecoveryPin,
};
use crate::rooms::room_v5_test_support::compatible_v5_room;
use crate::rooms::{
    PlayerIndex, RoomError, RoomStatus, StateHashEvaluation, StateRecoveryStartOutcome,
};
use std::time::{Duration, Instant};

#[test]
fn recovery_stays_on_old_epoch_until_exact_host_snapshot_is_pinned() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Playing);
    let now = Instant::now();
    let old_room_epoch = fixture.room.room_epoch;
    let old_session_epoch = fixture.room.session_epoch;
    let recovery = begin_recovery(&mut fixture, now);
    let manifest = manifest(recovery.recovery_id, recovery.repair_frame);

    assert_eq!(recovery.phase, StateRecoveryPhase::Preparing);
    assert_eq!(fixture.room.session_epoch, old_session_epoch);
    assert_eq!(fixture.room.status(), RoomStatus::RepairingState);

    assert_eq!(
        fixture.room.accept_v5_state_recovery_pin(
            fixture.guest_control,
            StateRecoveryPin {
                recovery_id: recovery.recovery_id,
                manifest: manifest.clone(),
            },
            SnapshotLimits::default(),
        ),
        Err(RoomError::HostOnly)
    );
    assert_eq!(fixture.room.session_epoch, old_session_epoch);

    let committed = fixture
        .room
        .accept_v5_state_recovery_pin(
            fixture.host_control,
            StateRecoveryPin {
                recovery_id: recovery.recovery_id,
                manifest: manifest.clone(),
            },
            SnapshotLimits::default(),
        )
        .expect("host pin commits recovery");

    assert_eq!(committed.phase, StateRecoveryPhase::Committed);
    assert_eq!(committed.pinned_snapshot, Some(manifest.clone()));
    assert_eq!(fixture.room.session_epoch, old_session_epoch + 1);
    assert_eq!(fixture.room.status(), RoomStatus::CheckingCompatibility);
    assert_eq!(fixture.room.sync_start_frame(), recovery.repair_frame);
    assert!(fixture.room.next_input_frames.is_empty());
    assert!(fixture.room.last_input_frames.is_empty());
    assert_eq!(fixture.room.next_authoritative_state_hash_frame, 1_200);

    let duplicate = fixture
        .room
        .accept_v5_state_recovery_pin_for_epoch(
            fixture.host_control,
            old_room_epoch,
            old_session_epoch,
            StateRecoveryPin {
                recovery_id: recovery.recovery_id,
                manifest,
            },
            SnapshotLimits::default(),
        )
        .expect("duplicate host pin is idempotent");
    assert_eq!(duplicate, committed);
    assert_eq!(fixture.room.session_epoch, old_session_epoch + 1);
}

#[test]
fn recovery_rejects_wrong_transaction_and_any_later_snapshot_substitution() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Playing);
    let recovery = begin_recovery(&mut fixture, Instant::now());
    let pinned = manifest(recovery.recovery_id, recovery.repair_frame);

    assert_eq!(
        fixture.room.accept_v5_state_recovery_pin(
            fixture.host_control,
            StateRecoveryPin {
                recovery_id: recovery.recovery_id + 1,
                manifest: pinned.clone(),
            },
            SnapshotLimits::default(),
        ),
        Err(RoomError::SnapshotInvalid)
    );

    fixture
        .room
        .accept_v5_state_recovery_pin(
            fixture.host_control,
            StateRecoveryPin {
                recovery_id: recovery.recovery_id,
                manifest: pinned.clone(),
            },
            SnapshotLimits::default(),
        )
        .expect("valid pin");

    let mut substituted = pinned;
    substituted.snapshot_id = "substituted-snapshot".to_string();
    assert_eq!(
        fixture.room.validate_v5_recovery_snapshot(&substituted),
        Err(RoomError::SnapshotInvalid)
    );
}

#[test]
fn recovery_pin_timeout_closes_the_room_instead_of_staying_wedged() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Playing);
    let now = Instant::now();
    begin_recovery(&mut fixture, now);

    assert!(
        !fixture
            .room
            .close_expired_state_recovery(now + Duration::from_millis(9_999))
    );
    assert!(
        fixture
            .room
            .close_expired_state_recovery(now + Duration::from_secs(10))
    );
    assert_eq!(fixture.room.status(), RoomStatus::Closed);
}

#[test]
fn third_recovery_attempt_inside_one_minute_closes_the_room() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Playing);
    let now = Instant::now();

    for attempt in 0..2 {
        assert!(matches!(
            fixture.room.begin_v5_state_recovery(
                mismatch(600 + attempt * 600, char::from(b'a' + attempt as u8)),
                now + Duration::from_secs(attempt),
            ),
            Ok(StateRecoveryStartOutcome::Preparing(_))
        ));
        fixture.room.finish_v5_state_recovery();
        fixture.room.status = RoomStatus::Playing;
    }

    assert!(matches!(
        fixture
            .room
            .begin_v5_state_recovery(mismatch(1_800, 'c'), now + Duration::from_secs(2),),
        Ok(StateRecoveryStartOutcome::AttemptLimitExceeded(_))
    ));
    assert_eq!(fixture.room.status(), RoomStatus::Closed);
}

#[test]
fn recovery_attempt_budget_uses_a_rolling_window() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Playing);
    let now = Instant::now();

    assert!(matches!(
        fixture
            .room
            .begin_v5_state_recovery(mismatch(600, 'a'), now),
        Ok(StateRecoveryStartOutcome::Preparing(_))
    ));
    fixture.room.finish_v5_state_recovery();
    fixture.room.status = RoomStatus::Playing;

    assert!(matches!(
        fixture
            .room
            .begin_v5_state_recovery(mismatch(1_200, 'b'), now + Duration::from_secs(60),),
        Ok(StateRecoveryStartOutcome::Preparing(_))
    ));
    assert_eq!(fixture.room.status(), RoomStatus::RepairingState);
}

fn begin_recovery(
    fixture: &mut super::room_v5_test_support::V5RoomFixture,
    now: Instant,
) -> crate::protocol::StateRecoveryView {
    fixture
        .room
        .accept_state_hash(fixture.host_control, state_hash(600, 'a'), now)
        .expect("host digest");
    match fixture
        .room
        .accept_state_hash(fixture.guest_control, state_hash(600, 'b'), now)
        .expect("guest digest")
    {
        StateHashEvaluation::RecoveryPrepare(recovery) => recovery,
        other => panic!("expected recovery prepare, got {other:?}"),
    }
}

fn state_hash(frame: u64, fill: char) -> StateHashReport {
    StateHashReport {
        frame,
        sha256: fill.to_string().repeat(64),
    }
}

fn mismatch(frame: u64, fill: char) -> StateHashMismatchView {
    StateHashMismatchView {
        frame,
        repair_frame: frame,
        hashes: vec![PlayerStateHashView {
            player_index: PlayerIndex::ONE,
            sha256: fill.to_string().repeat(64),
        }],
        nearby_matches: Vec::new(),
    }
}

fn manifest(recovery_id: u64, repair_frame: u64) -> SnapshotManifest {
    SnapshotManifest {
        snapshot_id: format!("recovery-{recovery_id}"),
        repair_frame,
        total_bytes: 4,
        sha256: "c".repeat(64),
    }
}
