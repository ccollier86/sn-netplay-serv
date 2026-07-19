use crate::protocol::{HostFrameOpen, ScheduledSessionStart};
use crate::rooms::room_v5_test_support::{batch, v5_room};
use crate::rooms::{HostFrameOpenOutcome, PlayerIndex, RoomError, RoomStatus};

#[test]
fn exact_host_open_releases_once_and_duplicate_is_idempotent() {
    let mut fixture = v5_room(RoomStatus::Playing);
    accept_host_input_zero(&mut fixture);
    let open = host_open(&fixture, 0);

    let HostFrameOpenOutcome::Released(release) = fixture
        .room
        .accept_host_frame_open(fixture.host_input, open, 100)
        .expect("host open")
    else {
        panic!("expected release");
    };
    assert_eq!(release.released_frame, 0);
    assert_eq!(release.next_host_frame, 1);

    let HostFrameOpenOutcome::Duplicate(duplicate) = fixture
        .room
        .accept_host_frame_open(fixture.host_input, open, 101)
        .expect("duplicate open")
    else {
        panic!("expected duplicate release");
    };
    assert_eq!(duplicate, release);
    assert_eq!(fixture.room.next_release_frame, 1);
}

#[test]
fn host_open_requires_host_ownership_exact_sequence_and_accepted_input() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let open = host_open(&fixture, 0);
    assert!(matches!(
        fixture
            .room
            .accept_host_frame_open(fixture.guest_input, open, 100),
        Err(RoomError::HostOnly)
    ));
    assert!(matches!(
        fixture
            .room
            .accept_host_frame_open(fixture.host_input, open, 100),
        Err(RoomError::OutOfOrderFrame)
    ));

    accept_host_input_zero(&mut fixture);
    assert!(matches!(
        fixture
            .room
            .accept_host_frame_open(fixture.host_input, host_open(&fixture, 1), 100),
        Err(RoomError::OutOfOrderFrame)
    ));
}

#[test]
fn scheduled_first_open_is_held_until_server_deadline() {
    let mut fixture = v5_room(RoomStatus::StartScheduled);
    fixture.room.scheduled_start = Some(ScheduledSessionStart {
        room_epoch: 1,
        session_epoch: 1,
        start_frame: 0,
        server_time_ms: 1_000,
        created_at_server_time_ms: 100,
        minimum_start_delay_ms: 900,
        clock_uncertainty_budget_ms: 0,
    });
    accept_host_input_zero(&mut fixture);

    assert!(matches!(
        fixture
            .room
            .accept_host_frame_open(fixture.host_input, host_open(&fixture, 0), 999),
        Ok(HostFrameOpenOutcome::Pending { delay_ms: 1 })
    ));
    assert!(fixture.room.release_due_v5_host_frame(999).is_none());
    let release = fixture
        .room
        .release_due_v5_host_frame(1_000)
        .expect("scheduled release");
    assert_eq!(release.released_frame, 0);
    assert_eq!(fixture.room.status, RoomStatus::Playing);
}

#[test]
fn periodic_legacy_clock_cannot_advance_a_v5_room() {
    let mut fixture = v5_room(RoomStatus::Playing);
    accept_host_input_zero(&mut fixture);
    assert!(fixture.room.release_next_server_frame(1_000).is_none());
    assert_eq!(fixture.room.next_release_frame, 0);
    assert_eq!(fixture.room.released_frame, None);
}

fn accept_host_input_zero(fixture: &mut super::room_v5_test_support::V5RoomFixture) {
    let input = batch(fixture, PlayerIndex::ONE, 0, &[1]);
    fixture
        .room
        .accept_strict_input_batch(fixture.host_input, input)
        .expect("host input");
}

fn host_open(fixture: &super::room_v5_test_support::V5RoomFixture, frame: u64) -> HostFrameOpen {
    HostFrameOpen {
        room_epoch: fixture.room.room_epoch,
        session_epoch: fixture.room.session_epoch,
        frame,
    }
}
