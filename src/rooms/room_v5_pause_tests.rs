use crate::protocol::{
    HostFrameOpen, InputCursorNackReason, InputCursorResponse, SessionPauseReason,
};
use crate::rooms::room_v5_test_support::{batch, v5_room};
use crate::rooms::{
    HostFrameOpenOutcome, PlayerIndex, RoomError, RoomStatus, SessionPauseReachedOutcome,
    SessionResumeOutcome,
};

#[test]
fn resume_bumps_epoch_clears_future_work_and_schedules_frame_after_pause() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let old_epoch_input = batch(&fixture, PlayerIndex::ONE, 0, &[1]);
    fixture
        .room
        .accept_strict_input_batch(fixture.host_input, old_epoch_input.clone())
        .expect("pre-pause input");
    fixture.room.pending_host_frame_open = Some(0);
    let old_session_epoch = fixture.room.session_epoch;

    let pause = fixture
        .room
        .request_session_pause(fixture.host_control, SessionPauseReason::Menu, 0)
        .expect("pause");
    assert_eq!(pause.pause_at_frame, 8);
    complete_pause_boundary(&mut fixture, pause.pause_at_frame);
    assert!(matches!(
        fixture.room.mark_session_pause_reached_with_outcome_at(
            fixture.host_control,
            pause.sequence,
            pause.pause_at_frame,
            1_000,
        ),
        Ok(SessionPauseReachedOutcome::Pausing(_))
    ));
    assert!(matches!(
        fixture.room.mark_session_pause_reached_with_outcome_at(
            fixture.guest_control,
            pause.sequence,
            pause.pause_at_frame,
            1_000,
        ),
        Ok(SessionPauseReachedOutcome::Paused(_))
    ));

    let outcome = fixture
        .room
        .request_session_resume_with_id_at(
            fixture.host_control,
            "resume-1".to_string(),
            SessionPauseReason::Menu,
            pause.sequence,
            1_000,
        )
        .expect("resume");
    let SessionResumeOutcome::ResumedV5 { scheduled_start } = outcome else {
        panic!("expected v5 scheduled resume");
    };

    assert_eq!(fixture.room.session_epoch, old_session_epoch + 1);
    assert_eq!(scheduled_start.session_epoch, old_session_epoch + 1);
    assert_eq!(scheduled_start.start_frame, pause.pause_at_frame + 1);
    assert!(scheduled_start.server_time_ms > 1_000);
    assert_eq!(fixture.room.status(), RoomStatus::StartScheduled);
    assert!(fixture.room.last_input_frames.is_empty());
    assert!(fixture.room.next_input_frames.is_empty());
    assert!(fixture.room.pending_host_frame_open.is_none());
    let old_epoch = fixture
        .room
        .accept_strict_input_batch(fixture.host_input, old_epoch_input)
        .expect("stale transition input remains nonfatal");
    assert!(matches!(
        old_epoch.response,
        InputCursorResponse::Nack(nack)
            if nack.session_epoch == old_session_epoch + 1
                && nack.expected_frame == pause.pause_at_frame + 1
                && nack.reason == InputCursorNackReason::SessionState
    ));
}

#[test]
fn canonical_resume_frame_is_hashed_without_changing_recovery_cursor_semantics() {
    let mut canonical = v5_room(RoomStatus::Playing);
    canonical.room.begin_v5_pause_resume(600, 1_000);
    assert_eq!(canonical.room.next_authoritative_state_hash_frame, 600);

    let mut after_canonical = v5_room(RoomStatus::Playing);
    after_canonical.room.begin_v5_pause_resume(601, 1_000);
    assert_eq!(
        after_canonical.room.next_authoritative_state_hash_frame,
        1_200
    );
}

#[test]
fn early_holder_release_auto_schedules_after_both_exact_acks() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let pause = fixture
        .room
        .request_session_pause(fixture.host_control, SessionPauseReason::Menu, 20)
        .expect("pause");
    assert!(matches!(
        fixture.room.request_session_resume_with_id_at(
            fixture.host_control,
            "resume-early".to_string(),
            SessionPauseReason::Menu,
            pause.sequence,
            2_000,
        ),
        Ok(SessionResumeOutcome::StillPaused(_))
    ));
    complete_pause_boundary(&mut fixture, pause.pause_at_frame);
    fixture
        .room
        .mark_session_pause_reached_with_outcome_at(
            fixture.host_control,
            pause.sequence,
            pause.pause_at_frame,
            2_000,
        )
        .expect("host ack");
    assert!(matches!(
        fixture.room.mark_session_pause_reached_with_outcome_at(
            fixture.guest_control,
            pause.sequence,
            pause.pause_at_frame,
            2_000,
        ),
        Ok(SessionPauseReachedOutcome::ResumedV5 { scheduled_start, .. })
            if scheduled_start.start_frame == pause.pause_at_frame + 1
    ));
}

#[test]
fn v5_rejects_an_inexact_pause_ack() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let pause = fixture
        .room
        .request_session_pause(fixture.host_control, SessionPauseReason::Menu, 0)
        .expect("pause");
    complete_pause_boundary(&mut fixture, pause.pause_at_frame);

    assert_eq!(
        fixture.room.mark_session_pause_reached_with_outcome_at(
            fixture.host_control,
            pause.sequence,
            pause.pause_at_frame + 1,
            1_000,
        ),
        Err(RoomError::RoomNotReady)
    );
}

#[test]
fn v5_pause_ack_waits_for_release_and_every_input_cursor_through_boundary() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let pause = fixture
        .room
        .request_session_pause(fixture.host_control, SessionPauseReason::Menu, 0)
        .expect("pause");

    assert_eq!(
        fixture.room.mark_session_pause_reached_with_outcome_at(
            fixture.host_control,
            pause.sequence,
            pause.pause_at_frame,
            1_000,
        ),
        Err(RoomError::RoomNotReady)
    );
    assert!(
        fixture
            .room
            .pause_state
            .as_ref()
            .expect("pause state")
            .view(crate::protocol::SessionPauseState::Pausing)
            .acknowledged_player_indexes
            .is_empty()
    );

    complete_pause_boundary(&mut fixture, pause.pause_at_frame);
    assert!(matches!(
        fixture.room.mark_session_pause_reached_with_outcome_at(
            fixture.host_control,
            pause.sequence,
            pause.pause_at_frame,
            1_000,
        ),
        Ok(SessionPauseReachedOutcome::Pausing(_))
    ));
}

#[test]
fn queued_host_opens_after_the_pause_boundary_are_ignored() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let input = batch(&fixture, PlayerIndex::ONE, 0, &[1, 2, 3, 4, 5, 6, 7, 8, 9]);
    fixture
        .room
        .accept_strict_input_batch(fixture.host_input, input)
        .expect("host input");
    let pause = fixture
        .room
        .request_session_pause(fixture.host_control, SessionPauseReason::Menu, 0)
        .expect("pause");

    for frame in 0..=pause.pause_at_frame {
        let open = host_open(&fixture, frame);
        assert!(matches!(
            fixture
                .room
                .accept_host_frame_open(fixture.host_input, open, 100),
            Ok(HostFrameOpenOutcome::Released(_))
        ));
    }
    for frame in pause.pause_at_frame + 1..=pause.pause_at_frame + 3 {
        assert!(matches!(
            fixture.room.accept_host_frame_open(
                fixture.host_input,
                host_open(&fixture, frame),
                100
            ),
            Ok(HostFrameOpenOutcome::IgnoredTransitionBoundary)
        ));
    }
    assert_eq!(fixture.room.next_release_frame, pause.pause_at_frame + 1);
}

fn host_open(fixture: &super::room_v5_test_support::V5RoomFixture, frame: u64) -> HostFrameOpen {
    HostFrameOpen {
        room_epoch: fixture.room.room_epoch,
        session_epoch: fixture.room.session_epoch,
        frame,
    }
}

fn complete_pause_boundary(
    fixture: &mut super::room_v5_test_support::V5RoomFixture,
    pause_at_frame: u64,
) {
    for (player_index, connection_id) in [
        (PlayerIndex::ONE, fixture.host_input),
        (PlayerIndex::TWO, fixture.guest_input),
    ] {
        let start_frame = fixture
            .room
            .next_input_frames
            .get(&player_index)
            .copied()
            .unwrap_or(fixture.room.sync_start_frame);
        if start_frame <= pause_at_frame {
            let fills = vec![1; usize::try_from(pause_at_frame - start_frame + 1).unwrap()];
            let input = batch(fixture, player_index, start_frame, &fills);
            fixture
                .room
                .accept_strict_input_batch(connection_id, input)
                .expect("pause boundary input");
        }
    }

    for frame in fixture.room.next_release_frame..=pause_at_frame {
        let open = host_open(fixture, frame);
        assert!(matches!(
            fixture
                .room
                .accept_host_frame_open(fixture.host_input, open, 100),
            Ok(HostFrameOpenOutcome::Released(_))
        ));
    }
}
