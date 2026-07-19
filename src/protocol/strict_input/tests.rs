use super::*;
use crate::limits::V5_RETROPAD_INPUT_BYTES;
use crate::rooms::PlayerIndex;

#[test]
fn round_trips_every_v5_input_lane_message() {
    let batch = fixture_batch();
    assert_eq!(
        decode_strict_input_batch(&encode_strict_input_batch(&batch).expect("batch")),
        Ok(batch)
    );

    let ack = fixture_ack();
    assert_eq!(
        decode_input_cursor_ack(&encode_input_cursor_ack(&ack)),
        Ok(ack)
    );

    let nack = fixture_nack();
    assert_eq!(
        decode_input_cursor_nack(&encode_input_cursor_nack(&nack)),
        Ok(nack)
    );

    let open = fixture_open();
    assert_eq!(
        decode_host_frame_open(&encode_host_frame_open(&open)),
        Ok(open)
    );

    let release = fixture_release();
    assert_eq!(
        decode_server_frame_release_v5(&encode_server_frame_release_v5(&release).expect("release")),
        Ok(release)
    );
}

#[test]
fn canonical_hex_fixtures_match_every_encoder_and_decoder() {
    assert_fixture(
        include_str!("../../../spec/netplay-v5/fixtures/strict-input-batch.hex"),
        &encode_strict_input_batch(&fixture_batch()).expect("batch"),
    );
    assert_fixture(
        include_str!("../../../spec/netplay-v5/fixtures/input-cursor-ack.hex"),
        &encode_input_cursor_ack(&fixture_ack()),
    );
    assert_fixture(
        include_str!("../../../spec/netplay-v5/fixtures/input-cursor-nack.hex"),
        &encode_input_cursor_nack(&fixture_nack()),
    );
    assert_fixture(
        include_str!("../../../spec/netplay-v5/fixtures/host-frame-open.hex"),
        &encode_host_frame_open(&fixture_open()),
    );
    assert_fixture(
        include_str!("../../../spec/netplay-v5/fixtures/server-frame-release.hex"),
        &encode_server_frame_release_v5(&fixture_release()).expect("release"),
    );
}

#[test]
fn strict_batch_rejects_wrong_payload_size_and_trailing_bytes() {
    let mut encoded = encode_strict_input_batch(&fixture_batch()).expect("batch");
    encoded[24] = 9;
    assert_eq!(
        decode_strict_input_batch(&encoded),
        Err(StrictInputCodecError::InvalidPayloadSize)
    );

    let mut encoded = encode_strict_input_batch(&fixture_batch()).expect("batch");
    encoded.push(0);
    assert_eq!(
        decode_strict_input_batch(&encoded),
        Err(StrictInputCodecError::Malformed)
    );
}

#[test]
fn release_rejects_duplicate_player_cursors() {
    let duplicate = ServerFrameReleaseV5 {
        accepted_inputs: vec![
            AcceptedInputCursor {
                player_index: PlayerIndex::ONE,
                next_expected_frame: 44,
            },
            AcceptedInputCursor {
                player_index: PlayerIndex::ONE,
                next_expected_frame: 45,
            },
        ],
        ..fixture_release()
    };
    assert_eq!(
        encode_server_frame_release_v5(&duplicate),
        Err(StrictInputCodecError::InvalidCursors)
    );
}

fn fixture_batch() -> StrictInputBatch {
    StrictInputBatch {
        room_epoch: 0x0102_0304_0506_0708,
        session_epoch: 0x1112_1314_1516_1718,
        player_index: PlayerIndex::TWO,
        start_frame: 42,
        payloads: vec![
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            [16, 32, 48, 64, 80, 96, 112, 128, 144, 160],
        ],
    }
}

fn fixture_ack() -> InputCursorAck {
    InputCursorAck {
        room_epoch: 0x0102_0304_0506_0708,
        session_epoch: 0x1112_1314_1516_1718,
        player_index: PlayerIndex::TWO,
        next_expected_frame: 44,
    }
}

fn fixture_nack() -> InputCursorNack {
    InputCursorNack {
        room_epoch: 0x0102_0304_0506_0708,
        session_epoch: 0x1112_1314_1516_1718,
        player_index: PlayerIndex::TWO,
        expected_frame: 44,
        received_frame: 46,
        reason: InputCursorNackReason::InputGap,
    }
}

fn fixture_open() -> HostFrameOpen {
    HostFrameOpen {
        room_epoch: 0x0102_0304_0506_0708,
        session_epoch: 0x1112_1314_1516_1718,
        frame: 42,
    }
}

fn fixture_release() -> ServerFrameReleaseV5 {
    ServerFrameReleaseV5 {
        room_epoch: 0x0102_0304_0506_0708,
        session_epoch: 0x1112_1314_1516_1718,
        released_frame: 42,
        next_host_frame: 43,
        accepted_inputs: vec![
            AcceptedInputCursor {
                player_index: PlayerIndex::ONE,
                next_expected_frame: 44,
            },
            AcceptedInputCursor {
                player_index: PlayerIndex::TWO,
                next_expected_frame: 45,
            },
        ],
    }
}

fn assert_fixture(hex: &str, encoded: &[u8]) {
    assert_eq!(decode_hex(hex), encoded);
}

fn decode_hex(value: &str) -> Vec<u8> {
    let value = value.trim();
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).expect("fixture hex"))
        .collect()
}

const _: [(); V5_RETROPAD_INPUT_BYTES] = [(); 10];
