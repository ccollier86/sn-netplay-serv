use crate::protocol::{
    ClientNetworkQualityReport, ClockSyncSample, StateDigestMode, StateHashReport,
    V5_INPUT_CODEC_ID,
};
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
            state_hash(600, 'a'),
            std::time::Instant::now(),
        )
        .expect("host digest");
    assert!(matches!(
        fixture.room.accept_state_hash(
            fixture.guest_control,
            state_hash(600, 'b'),
            std::time::Instant::now()
        ),
        Ok(StateHashEvaluation::RecoveryPrepare(_))
    ));
    assert_eq!(fixture.room.session_epoch, session_epoch);
    assert_eq!(fixture.room.status(), RoomStatus::RepairingState);
}

#[test]
fn authoritative_v5_accepts_only_exact_canonical_checkpoints() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Playing);
    let now = std::time::Instant::now();

    assert_eq!(
        fixture
            .room
            .accept_state_hash(fixture.host_control, state_hash(599, 'a'), now),
        Err(RoomError::InvalidPayload)
    );
    assert_eq!(
        fixture
            .room
            .accept_state_hash(fixture.host_control, state_hash(1_200, 'a'), now),
        Err(RoomError::InvalidPayload)
    );
    assert_eq!(
        fixture
            .room
            .accept_state_hash(fixture.host_control, state_hash(600, 'a'), now),
        Ok(StateHashEvaluation::Pending)
    );
    assert_eq!(
        fixture
            .room
            .accept_state_hash(fixture.guest_control, state_hash(600, 'a'), now),
        Ok(StateHashEvaluation::Matched(600))
    );
    assert_eq!(fixture.room.next_authoritative_state_hash_frame, 1_200);
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

#[test]
fn v5_startup_delay_uses_both_fresh_paths_and_negotiated_frame_rate() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Ready);
    let now = std::time::Instant::now();
    let report = ClientNetworkQualityReport {
        round_trip_ms: Some(40),
        jitter_ms: Some(0),
        ..ClientNetworkQualityReport::default()
    };
    fixture
        .room
        .record_network_report(fixture.host_control, None, Some(report.clone()), now);
    fixture
        .room
        .record_network_report(fixture.guest_control, None, Some(report), now);

    fixture.room.apply_initial_v5_input_delay(now);

    assert_eq!(fixture.room.session.controller.input_delay_frames, 4);
}

#[test]
fn v5_startup_delay_keeps_configured_default_without_both_fresh_reports() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Ready);
    let now = std::time::Instant::now();
    fixture.room.record_network_report(
        fixture.host_control,
        None,
        Some(ClientNetworkQualityReport {
            round_trip_ms: Some(120),
            jitter_ms: Some(20),
            ..ClientNetworkQualityReport::default()
        }),
        now,
    );

    fixture.room.apply_initial_v5_input_delay(now);

    assert_eq!(fixture.room.session.controller.input_delay_frames, 3);
}

#[test]
fn v5_startup_delay_rejects_incomplete_and_prior_preparation_reports() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Ready);
    let now = std::time::Instant::now();
    let incomplete = ClientNetworkQualityReport {
        round_trip_ms: Some(120),
        jitter_ms: None,
        ..ClientNetworkQualityReport::default()
    };
    fixture
        .room
        .record_network_report(fixture.host_control, None, Some(incomplete.clone()), now);
    fixture
        .room
        .record_network_report(fixture.guest_control, None, Some(incomplete), now);
    fixture.room.apply_initial_v5_input_delay(now);
    assert_eq!(fixture.room.session.controller.input_delay_frames, 3);

    let complete = ClientNetworkQualityReport {
        round_trip_ms: Some(120),
        jitter_ms: Some(20),
        ..ClientNetworkQualityReport::default()
    };
    fixture
        .room
        .record_network_report(fixture.host_control, None, Some(complete.clone()), now);
    fixture
        .room
        .record_network_report(fixture.guest_control, None, Some(complete), now);
    fixture.room.reset_sync_state_to(600);
    fixture.room.apply_initial_v5_input_delay(now);

    assert_eq!(fixture.room.session.controller.input_delay_frames, 3);
    assert!(
        fixture
            .room
            .players
            .iter()
            .all(|slot| slot.latest_network_report.is_none())
    );
}

#[test]
fn v5_clock_sync_generation_rejects_prior_preparation_samples() {
    let mut fixture = compatible_v5_room(StateDigestMode::Authoritative, RoomStatus::Ready);
    fixture.room.players.iter_mut().for_each(|slot| {
        slot.supports_scheduled_start = true;
        slot.supports_clock_sync = true;
    });
    let old_request = fixture.room.request_clock_sync_sample(100);
    fixture.room.reset_start_sync_state();
    let current_request = fixture.room.request_clock_sync_sample(200);

    assert_ne!(old_request.request_id, current_request.request_id);
    assert_eq!(
        fixture.room.accept_clock_sync_sample(
            fixture.host_control,
            ClockSyncSample {
                request_id: old_request.request_id,
                sample_index: 0,
                server_send_time_ms: old_request.server_send_time_ms,
                client_receive_time_ms: 110,
                client_send_time_ms: 112,
            },
            std::time::Instant::now(),
            210,
        ),
        Err(RoomError::InvalidPayload),
    );
    assert!(fixture.room.clock_sample_indices_by_player.is_empty());
}

fn state_hash(frame: u64, fill: char) -> StateHashReport {
    StateHashReport {
        frame,
        sha256: fill.to_string().repeat(64),
    }
}
