use crate::protocol::{InputCursorNackReason, InputCursorResponse};
use crate::rooms::room_v5_test_support::{batch, v5_room};
use crate::rooms::{PlayerIndex, RoomStatus};

#[test]
fn exact_input_advances_cumulative_cursor_and_returns_new_payload() {
    let mut fixture = v5_room(RoomStatus::SyncingState);
    let input = batch(&fixture, PlayerIndex::ONE, 0, &[1, 2]);
    let outcome = fixture
        .room
        .accept_strict_input_batch(fixture.host_input, input)
        .expect("input accepted");

    let InputCursorResponse::Ack(ack) = outcome.response else {
        panic!("expected ACK");
    };
    assert_eq!(ack.next_expected_frame, 2);
    let accepted = outcome.accepted_batch.expect("new input");
    assert_eq!(accepted.start_frame, 0);
    assert_eq!(accepted.payloads, vec![[1; 10], [2; 10]]);
}

#[test]
fn duplicate_prefix_is_idempotent_and_only_new_suffix_is_relayed() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let initial = batch(&fixture, PlayerIndex::ONE, 0, &[1, 2]);
    fixture
        .room
        .accept_strict_input_batch(fixture.host_input, initial)
        .expect("initial input");

    let resend = batch(&fixture, PlayerIndex::ONE, 0, &[1, 2, 3]);
    let outcome = fixture
        .room
        .accept_strict_input_batch(fixture.host_input, resend)
        .expect("resend");
    let InputCursorResponse::Ack(ack) = outcome.response else {
        panic!("expected ACK");
    };
    assert_eq!(ack.next_expected_frame, 3);
    let accepted = outcome.accepted_batch.expect("new suffix");
    assert_eq!(accepted.start_frame, 2);
    assert_eq!(accepted.payloads, vec![[3; 10]]);

    let old = batch(&fixture, PlayerIndex::ONE, 0, &[1, 2]);
    let duplicate = fixture
        .room
        .accept_strict_input_batch(fixture.host_input, old)
        .expect("old duplicate");
    assert!(duplicate.accepted_batch.is_none());
    assert!(matches!(
        duplicate.response,
        InputCursorResponse::Ack(ack) if ack.next_expected_frame == 3
    ));
}

#[test]
fn future_gap_nacks_without_mutating_the_expected_cursor() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let gap = batch(&fixture, PlayerIndex::ONE, 2, &[3]);
    let outcome = fixture
        .room
        .accept_strict_input_batch(fixture.host_input, gap)
        .expect("shape-valid gap");

    assert!(outcome.accepted_batch.is_none());
    assert!(matches!(
        outcome.response,
        InputCursorResponse::Nack(nack)
            if nack.expected_frame == 0
                && nack.received_frame == 2
                && nack.reason == InputCursorNackReason::InputGap
    ));

    let next = batch(&fixture, PlayerIndex::ONE, 0, &[1]);
    let exact = fixture
        .room
        .accept_strict_input_batch(fixture.host_input, next)
        .expect("cursor remained exact");
    assert!(matches!(
        exact.response,
        InputCursorResponse::Ack(ack) if ack.next_expected_frame == 1
    ));
}

#[test]
fn future_bound_nacks_without_accepting_frame_97() {
    let mut fixture = v5_room(RoomStatus::Playing);
    fixture.room.next_input_frames.insert(PlayerIndex::ONE, 97);
    fixture.room.last_input_frames.insert(PlayerIndex::ONE, 96);

    let future = batch(&fixture, PlayerIndex::ONE, 97, &[9]);
    let outcome = fixture
        .room
        .accept_strict_input_batch(fixture.host_input, future)
        .expect("bounded rejection");
    assert!(matches!(
        outcome.response,
        InputCursorResponse::Nack(nack)
            if nack.expected_frame == 97
                && nack.received_frame == 97
                && nack.reason == InputCursorNackReason::FutureFrameTooLarge
    ));
    assert_eq!(
        fixture.room.next_input_frames.get(&PlayerIndex::ONE),
        Some(&97)
    );
}

#[test]
fn wrong_player_ownership_is_terminal_instead_of_becoming_a_nack() {
    let mut fixture = v5_room(RoomStatus::Playing);
    let spoofed = batch(&fixture, PlayerIndex::TWO, 0, &[1]);
    let result = fixture
        .room
        .accept_strict_input_batch(fixture.host_input, spoofed);
    let Err(error) = result else {
        panic!("slot spoofing must be rejected");
    };
    assert_eq!(
        error,
        crate::rooms::RoomError::SlotSpoofing(PlayerIndex::TWO)
    );
}
