use crate::protocol::{StateDigestMode, StateHashReport, V5_INPUT_CODEC_ID};
use crate::rooms::room_v5_test_support::{compatible_v5_room, fingerprint, v5_room};
use crate::rooms::{RoomError, RoomStatus, StateHashEvaluation};

#[test]
fn v5_requires_a_complete_determinism_profile() {
    let mut fixture = v5_room(RoomStatus::CheckingCompatibility);
    let mut missing = fingerprint(StateDigestMode::Diagnostic, "android-a");
    missing.determinism_v5 = None;

    assert_eq!(
        fixture
            .room
            .set_compatibility_for_connection(fixture.host_control, missing),
        Err(RoomError::CompatibilityMismatch)
    );
}

#[test]
fn local_artifacts_may_differ_when_curated_determinism_identity_matches() {
    let mut fixture = v5_room(RoomStatus::CheckingCompatibility);
    fixture
        .room
        .set_compatibility_for_connection(
            fixture.host_control,
            fingerprint(StateDigestMode::Diagnostic, "android-build-a"),
        )
        .expect("host profile");
    fixture
        .room
        .set_compatibility_for_connection(
            fixture.guest_control,
            fingerprint(StateDigestMode::Diagnostic, "android-build-b"),
        )
        .expect("guest profile");

    assert_eq!(fixture.room.status(), RoomStatus::SyncingState);
}

#[test]
fn incompatible_codec_is_rejected_before_state_sync() {
    let mut fixture = v5_room(RoomStatus::CheckingCompatibility);
    let mut incompatible = fingerprint(StateDigestMode::Diagnostic, "android-a");
    incompatible
        .determinism_v5
        .as_mut()
        .expect("v5 profile")
        .input_codec_id = format!("{V5_INPUT_CODEC_ID}-unknown");

    assert_eq!(
        fixture
            .room
            .set_compatibility_for_connection(fixture.host_control, incompatible),
        Err(RoomError::CompatibilityMismatch)
    );
}

#[test]
fn diagnostic_digest_mismatch_never_resets_the_session() {
    let mut fixture = compatible_v5_room(StateDigestMode::Diagnostic, RoomStatus::Playing);
    let session_epoch = fixture.room.session_epoch;
    assert_eq!(
        fixture
            .room
            .accept_state_hash(
                fixture.host_control,
                state_hash(60, 'a'),
                std::time::Instant::now(),
            )
            .expect("host digest"),
        StateHashEvaluation::Pending
    );
    assert!(matches!(
        fixture.room.accept_state_hash(
            fixture.guest_control,
            state_hash(60, 'b'),
            std::time::Instant::now()
        ),
        Ok(StateHashEvaluation::TrueMismatch(_))
    ));
    assert_eq!(fixture.room.session_epoch, session_epoch);
    assert_eq!(fixture.room.status(), RoomStatus::Playing);
}

#[test]
fn authoritative_digest_mismatch_enters_recovery() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Playing);
    let session_epoch = fixture.room.session_epoch;
    fixture
        .room
        .accept_state_hash(
            fixture.host_control,
            state_hash(60, 'a'),
            std::time::Instant::now(),
        )
        .expect("host digest");
    assert!(matches!(
        fixture.room.accept_state_hash(
            fixture.guest_control,
            state_hash(60, 'b'),
            std::time::Instant::now()
        ),
        Ok(StateHashEvaluation::RecoveryPrepare(_))
    ));
    assert_eq!(fixture.room.session_epoch, session_epoch);
    assert_eq!(fixture.room.status(), RoomStatus::RepairingState);
}

#[test]
fn scheduled_epoch_ignores_boundary_hash_until_host_frame_open() {
    let mut fixture =
        compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::StartScheduled);
    let session_epoch = fixture.room.session_epoch;

    assert_eq!(
        fixture
            .room
            .accept_state_hash(
                fixture.host_control,
                state_hash(1_200, 'a'),
                std::time::Instant::now(),
            )
            .expect("host boundary digest"),
        StateHashEvaluation::Pending
    );
    assert_eq!(
        fixture
            .room
            .accept_state_hash(
                fixture.guest_control,
                state_hash(1_200, 'b'),
                std::time::Instant::now(),
            )
            .expect("guest boundary digest"),
        StateHashEvaluation::Pending
    );
    assert_eq!(fixture.room.session_epoch, session_epoch);
    assert_eq!(fixture.room.status(), RoomStatus::StartScheduled);
}

#[test]
fn disabled_digest_reports_are_ignored() {
    let mut fixture = compatible_v5_room(StateDigestMode::Disabled, RoomStatus::Playing);
    assert_eq!(
        fixture
            .room
            .accept_state_hash(
                fixture.host_control,
                state_hash(60, 'a'),
                std::time::Instant::now(),
            )
            .expect("disabled digest"),
        StateHashEvaluation::Disabled
    );
}

fn state_hash(frame: u64, fill: char) -> StateHashReport {
    StateHashReport {
        frame,
        sha256: fill.to_string().repeat(64),
    }
}
