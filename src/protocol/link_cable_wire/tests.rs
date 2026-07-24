use super::*;

const MANIFEST: &str = include_str!("../../../protocol/link-cable-v1/manifest.json");
const GBA_V2_MANIFEST: &str = include_str!("../../../protocol/link-cable-v2/manifest.json");

#[test]
fn copied_manifest_and_hex_inventory_are_cross_checked_independently() {
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).expect("canonical manifest");
    assert_eq!(manifest["schemaVersion"], 1);
    assert_eq!(manifest["fixtureSet"], "shadowboy-link-cable-v1");
    assert_eq!(manifest["wireVersion"], u64::from(LINK_CABLE_WIRE_VERSION));
    assert_eq!(
        manifest["headerBytes"],
        u64::try_from(LINK_CABLE_WIRE_HEADER_BYTES).expect("header bytes")
    );
    assert_eq!(
        manifest["maxFrameBytes"],
        u64::try_from(MAX_LINK_CABLE_WIRE_BYTES).expect("maximum frame bytes")
    );
    assert_eq!(manifest["endianness"], "little");

    let fixtures = manifest["fixtures"].as_array().expect("fixture inventory");
    let expected_ids = fixture_cases()
        .into_iter()
        .map(|fixture| fixture.id)
        .collect::<Vec<_>>();
    let actual_ids = fixtures
        .iter()
        .map(|fixture| fixture["id"].as_str().expect("fixture id"))
        .collect::<Vec<_>>();
    assert_eq!(actual_ids, expected_ids);

    for fixture in fixtures {
        let id = fixture["id"].as_str().expect("fixture id");
        let file = fixture["file"].as_str().expect("fixture file");
        let expected = decode_hex(fixture["expectedHex"].as_str().expect("expected hex"));
        let copied_hex = fixture_hex(file);
        assert_eq!(copied_hex, expected, "{id}: manifest and hex file drifted");
        assert_eq!(
            copied_hex.len(),
            fixture["frameBytes"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .expect("frame bytes"),
            "{id}: frame byte count"
        );
        assert_eq!(
            copied_hex.len(),
            LINK_CABLE_WIRE_HEADER_BYTES
                + fixture["bodyBytes"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .expect("body bytes"),
            "{id}: body byte count"
        );
    }
}

#[test]
fn every_typed_encoder_and_decoder_matches_external_goldens() {
    for fixture in fixture_cases() {
        let expected = fixture_hex(fixture.file);
        assert_eq!(
            encode_link_cable_wire_frame(&fixture.frame),
            Ok(expected.clone()),
            "{}: generic encoder",
            fixture.id
        );
        assert_eq!(
            decode_link_cable_wire_frame(fixture.protocol, &expected),
            Ok(fixture.frame.clone()),
            "{}: generic decoder",
            fixture.id
        );

        match &fixture.frame {
            LinkCableWireFrame::GbaSioMulti(frame) => {
                assert_eq!(
                    encode_gba_sio_multi_frame(frame),
                    Ok(expected.clone()),
                    "{}: GBA encoder",
                    fixture.id
                );
                assert_eq!(
                    decode_gba_sio_multi_frame(&expected),
                    Ok(frame.clone()),
                    "{}: GBA decoder",
                    fixture.id
                );
            }
            LinkCableWireFrame::GbaSioMultiV2(frame) => {
                assert_eq!(
                    encode_gba_sio_multi_v2_frame(frame),
                    Ok(expected.clone()),
                    "{}: GBA v2 encoder",
                    fixture.id
                );
                assert_eq!(
                    decode_gba_sio_multi_v2_frame(&expected),
                    Ok(frame.clone()),
                    "{}: GBA v2 decoder",
                    fixture.id
                );
            }
            LinkCableWireFrame::GbSerial(frame) => {
                assert_eq!(
                    encode_gb_serial_frame(frame),
                    Ok(expected.clone()),
                    "{}: GB encoder",
                    fixture.id
                );
                assert_eq!(
                    decode_gb_serial_frame(&expected),
                    Ok(frame.clone()),
                    "{}: GB decoder",
                    fixture.id
                );
            }
        }
    }
}

#[test]
fn every_gba_v2_encoder_and_decoder_matches_the_shared_external_goldens() {
    let manifest: serde_json::Value =
        serde_json::from_str(GBA_V2_MANIFEST).expect("canonical GBA v2 manifest");
    assert_eq!(manifest["fixtureSet"], "shadowboy-link-cable-v2");
    assert_eq!(manifest["protocol"], "gba-sio-multi-v2");
    assert_eq!(manifest["wireVersion"], u64::from(LINK_CABLE_WIRE_VERSION));

    let fixtures = manifest["fixtures"].as_array().expect("v2 fixtures");
    let cases = gba_v2_fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for (fixture, case) in fixtures.iter().zip(cases) {
        assert_eq!(fixture["id"], case.id);
        assert_eq!(fixture["file"], case.file);
        let expected = v2_fixture_hex(case.file);
        assert_eq!(
            fixture["expectedHex"]
                .as_str()
                .map(decode_hex)
                .expect("v2 expected hex"),
            expected,
            "{}: manifest bytes",
            case.id
        );
        assert_eq!(
            fixture["frameBytes"].as_u64(),
            u64::try_from(expected.len()).ok(),
            "{}: frame bytes",
            case.id
        );
        assert_eq!(
            encode_gba_sio_multi_v2_frame(&case.frame),
            Ok(expected.clone()),
            "{}: v2 encoder",
            case.id
        );
        assert_eq!(
            encode_link_cable_wire_frame(&LinkCableWireFrame::GbaSioMultiV2(case.frame.clone())),
            Ok(expected.clone()),
            "{}: generic v2 encoder",
            case.id
        );
        assert_eq!(
            decode_gba_sio_multi_v2_frame(&expected),
            Ok(case.frame.clone()),
            "{}: v2 decoder",
            case.id
        );
        assert_eq!(
            decode_link_cable_wire_frame(LinkCableWireProtocol::GbaSioMultiV2, &expected),
            Ok(LinkCableWireFrame::GbaSioMultiV2(case.frame)),
            "{}: generic v2 decoder",
            case.id
        );
    }
}

#[test]
fn every_truncated_canonical_fixture_is_rejected_without_panicking() {
    for fixture in fixture_cases() {
        let canonical = fixture_hex(fixture.file);
        for prefix_length in 0..canonical.len() {
            assert!(
                decode_link_cable_wire_frame(fixture.protocol, &canonical[..prefix_length],)
                    .is_err(),
                "{} unexpectedly decoded a {prefix_length}-byte prefix of its {}-byte frame",
                fixture.id,
                canonical.len(),
            );
        }
    }
}

#[test]
fn every_non_fixture_gba_mode_set_value_round_trips_with_matching_registers() {
    let cases: [(u8, u16, u16); 5] = [
        (0, 0x0003, 0x0034),
        (1, 0x1003, 0x0034),
        (3, 0x3003, 0x0034),
        (8, 0x0003, 0x8034),
        (12, 0x0003, 0xc034),
    ];

    for (mode, siocnt, rcnt) in cases {
        let frame = GbaSioMultiFrame {
            header: gba_header(10 + u64::from(mode), 0),
            event: GbaSioMultiEvent::ModeSet {
                mode,
                siocnt,
                rcnt,
                emulated_time: 0x0102_0304_0506_0708,
            },
        };
        let encoded = encode_gba_sio_multi_frame(&frame).expect("valid GBA MODE_SET");
        assert_eq!(
            decode_gba_sio_multi_frame(&encoded),
            Ok(frame),
            "mode {mode} with SIOCNT {siocnt:#06x} and RCNT {rcnt:#06x}",
        );
    }
}

#[test]
fn decoder_rejects_malformed_shared_headers_before_body_admission() {
    let canonical = fixture_hex("gba-transfer-reply.hex");

    assert_eq!(
        decode_gba_sio_multi_frame(&canonical[..42]),
        Err(LinkCableWireCodecError::InvalidFrameSize)
    );
    assert_eq!(
        decode_gba_sio_multi_frame(&[0; MAX_LINK_CABLE_WIRE_BYTES + 1]),
        Err(LinkCableWireCodecError::InvalidFrameSize)
    );
    assert_gba_mutation(
        &canonical,
        LinkCableWireCodecError::UnsupportedMagic,
        |bytes| {
            bytes[0] = 0;
        },
    );
    assert_gba_mutation(
        &canonical,
        LinkCableWireCodecError::UnsupportedVersion,
        |bytes| bytes[4] = 2,
    );
    for flags_offset in [6, 7] {
        assert_gba_mutation(
            &canonical,
            LinkCableWireCodecError::ReservedFlagsSet,
            |bytes| bytes[flags_offset] = 1,
        );
    }
    for high_byte_offset in [15, 23, 31, 39] {
        assert_gba_mutation(&canonical, LinkCableWireCodecError::HighBitSet, |bytes| {
            bytes[high_byte_offset] |= 0x80
        });
    }
    assert_gba_mutation(&canonical, LinkCableWireCodecError::InvalidSlot, |bytes| {
        bytes[40] = 2;
    });
    assert_gba_mutation(
        &canonical,
        LinkCableWireCodecError::BodyLengthMismatch,
        |bytes| bytes[41] = 13,
    );

    let mut trailing = canonical.clone();
    trailing.push(0);
    assert_eq!(
        decode_gba_sio_multi_frame(&trailing),
        Err(LinkCableWireCodecError::BodyLengthMismatch)
    );
}

#[test]
fn decoder_requires_namespace_specific_kinds_and_exact_body_lengths() {
    let gba = fixture_hex("gba-transfer-reply.hex");
    for kind in [0, 6, u8::MAX] {
        assert_gba_mutation(
            &gba,
            LinkCableWireCodecError::UnsupportedEventKind,
            |bytes| bytes[5] = kind,
        );
    }

    let gb = fixture_hex("gb-serial-start-slot0.hex");
    assert_gb_mutation(
        &gb,
        LinkCableWireCodecError::UnsupportedEventKind,
        |bytes| bytes[5] = 1,
    );
    assert_eq!(
        decode_link_cable_wire_frame(
            LinkCableWireProtocol::GbSerialV1,
            &fixture_hex("gba-mode-set.hex"),
        ),
        Err(LinkCableWireCodecError::UnsupportedEventKind)
    );
    assert_eq!(
        decode_link_cable_wire_frame(LinkCableWireProtocol::GbaSioMultiV1, &gb),
        Err(LinkCableWireCodecError::InvalidEventBodyLength)
    );

    let mut short_gba_reply = gba.clone();
    short_gba_reply.pop();
    short_gba_reply[41] = 13;
    assert_eq!(
        decode_gba_sio_multi_frame(&short_gba_reply),
        Err(LinkCableWireCodecError::InvalidEventBodyLength)
    );

    let mut maximum_frame = gba;
    maximum_frame.resize(MAX_LINK_CABLE_WIRE_BYTES, 0);
    let body_length =
        u16::try_from(MAX_LINK_CABLE_WIRE_BYTES - LINK_CABLE_WIRE_HEADER_BYTES).expect("body");
    maximum_frame[41..43].copy_from_slice(&body_length.to_le_bytes());
    assert_eq!(
        decode_gba_sio_multi_frame(&maximum_frame),
        Err(LinkCableWireCodecError::InvalidEventBodyLength)
    );
}

#[test]
fn decoder_rejects_gba_body_invariants_and_wrong_event_roles() {
    let mode_set = fixture_hex("gba-mode-set.hex");
    assert_gba_mutation(
        &mode_set,
        LinkCableWireCodecError::InvalidGbaMode,
        |bytes| bytes[43] = 7,
    );
    assert_gba_mutation(
        &mode_set,
        LinkCableWireCodecError::InvalidGbaMode,
        |bytes| bytes[43] = 3,
    );
    assert_gba_mutation(&mode_set, LinkCableWireCodecError::HighBitSet, |bytes| {
        *bytes.last_mut().expect("time") |= 0x80;
    });

    let start = fixture_hex("gba-transfer-start.hex");
    assert_gba_mutation(
        &start,
        LinkCableWireCodecError::InvalidTransferId,
        zero_transfer_id,
    );
    assert_gba_mutation(
        &start,
        LinkCableWireCodecError::InvalidGbaTransferStart,
        |bytes| bytes[47] = 3,
    );
    assert_gba_mutation(
        &start,
        LinkCableWireCodecError::InvalidGbaTransferStart,
        |bytes| bytes[49] = 0x70,
    );
    assert_gba_mutation(
        &start,
        LinkCableWireCodecError::InvalidGbaTransferStart,
        |bytes| bytes[48] |= 0x80,
    );
    assert_gba_mutation(&start, LinkCableWireCodecError::InvalidEventRole, |bytes| {
        bytes[40] = 1
    });

    let reply = fixture_hex("gba-transfer-reply.hex");
    assert_gba_mutation(&reply, LinkCableWireCodecError::InvalidEventRole, |bytes| {
        bytes[40] = 0
    });
    assert_gba_mutation(&reply, LinkCableWireCodecError::HighBitSet, |bytes| {
        *bytes.last_mut().expect("time") |= 0x80;
    });

    let commit = fixture_hex("gba-transfer-commit.hex");
    assert_gba_mutation(
        &commit,
        LinkCableWireCodecError::InvalidEventRole,
        |bytes| bytes[40] = 1,
    );
    assert_gba_mutation(
        &commit,
        LinkCableWireCodecError::InvalidGbaDisconnectedWords,
        |bytes| bytes[51] = 0,
    );
    assert_gba_mutation(
        &commit,
        LinkCableWireCodecError::InvalidGbaDisconnectedWords,
        |bytes| bytes[53] = 0,
    );

    let abort = fixture_hex("gba-transfer-abort.hex");
    for reason in [0, 6, u8::MAX] {
        assert_gba_mutation(
            &abort,
            LinkCableWireCodecError::InvalidAbortReason,
            |bytes| *bytes.last_mut().expect("reason") = reason,
        );
    }
}

#[test]
fn decoder_rejects_gb_body_invariants_and_dynamic_role_violations() {
    let start = fixture_hex("gb-serial-start-slot0.hex");
    assert_gb_mutation(
        &start,
        LinkCableWireCodecError::InvalidTransferId,
        zero_transfer_id,
    );
    assert_gb_mutation(&start, LinkCableWireCodecError::InvalidSlot, |bytes| {
        bytes[47] = 2;
    });
    assert_gb_mutation(&start, LinkCableWireCodecError::InvalidEventRole, |bytes| {
        bytes[40] = 1
    });
    for sc_control in [0, 0x01, 0x80, 0x82, 0x85, u8::MAX] {
        assert_gb_mutation(
            &start,
            LinkCableWireCodecError::InvalidGbSerialControl,
            |bytes| bytes[48] = sc_control,
        );
    }
    assert_gb_mutation(&start, LinkCableWireCodecError::HighBitSet, |bytes| {
        *bytes.last_mut().expect("time") |= 0x80;
    });

    let reply = fixture_hex("gb-serial-reply.hex");
    assert_gb_mutation(&reply, LinkCableWireCodecError::InvalidEventRole, |bytes| {
        bytes[40] = 0
    });
    assert_gb_mutation(&reply, LinkCableWireCodecError::InvalidSlot, |bytes| {
        bytes[47] = 2;
    });

    let commit = fixture_hex("gb-serial-commit.hex");
    assert_gb_mutation(
        &commit,
        LinkCableWireCodecError::InvalidEventRole,
        |bytes| bytes[40] = 1,
    );

    let abort = fixture_hex("gb-serial-abort.hex");
    assert_gb_mutation(&abort, LinkCableWireCodecError::InvalidSlot, |bytes| {
        bytes[47] = 2;
    });
    for reason in [0, 6, u8::MAX] {
        assert_gb_mutation(
            &abort,
            LinkCableWireCodecError::InvalidAbortReason,
            |bytes| *bytes.last_mut().expect("reason") = reason,
        );
    }
}

#[test]
fn encoder_enforces_the_same_header_body_and_role_invariants() {
    let mut gba = match fixture_case("gba-transfer-reply").frame {
        LinkCableWireFrame::GbaSioMulti(frame) => frame,
        LinkCableWireFrame::GbaSioMultiV2(_) | LinkCableWireFrame::GbSerial(_) => {
            panic!("GBA v1 fixture")
        }
    };
    gba.header.room_epoch = 1_u64 << 63;
    assert_eq!(
        encode_gba_sio_multi_frame(&gba),
        Err(LinkCableWireCodecError::HighBitSet)
    );
    gba.header.room_epoch = 2;
    gba.header.sender_slot = 2;
    assert_eq!(
        encode_gba_sio_multi_frame(&gba),
        Err(LinkCableWireCodecError::InvalidSlot)
    );
    gba.header.sender_slot = 1;
    gba.event = GbaSioMultiEvent::TransferReply {
        transfer_id: 0,
        child_word: 0,
        emulated_time: 0,
    };
    assert_eq!(
        encode_gba_sio_multi_frame(&gba),
        Err(LinkCableWireCodecError::InvalidTransferId)
    );
    gba.event = GbaSioMultiEvent::TransferReply {
        transfer_id: 1,
        child_word: 0,
        emulated_time: 1_u64 << 63,
    };
    assert_eq!(
        encode_gba_sio_multi_frame(&gba),
        Err(LinkCableWireCodecError::HighBitSet)
    );
    gba.header.sender_slot = 0;
    gba.event = GbaSioMultiEvent::TransferReply {
        transfer_id: 1,
        child_word: 0,
        emulated_time: 0,
    };
    assert_eq!(
        encode_gba_sio_multi_frame(&gba),
        Err(LinkCableWireCodecError::InvalidEventRole)
    );

    let mut gb = match fixture_case("gb-serial-start-slot0").frame {
        LinkCableWireFrame::GbSerial(frame) => frame,
        LinkCableWireFrame::GbaSioMulti(_) | LinkCableWireFrame::GbaSioMultiV2(_) => {
            panic!("GB fixture")
        }
    };
    gb.event = GbSerialEvent::Start {
        transfer_id: 1,
        clock_owner_slot: 1,
        sc_control: GB_SERIAL_FAST_CLOCK_CONTROL,
        owner_byte: 0,
        emulated_time: 0,
    };
    assert_eq!(
        encode_gb_serial_frame(&gb),
        Err(LinkCableWireCodecError::InvalidEventRole)
    );
    gb.event = GbSerialEvent::Start {
        transfer_id: 1,
        clock_owner_slot: 0,
        sc_control: 0x80,
        owner_byte: 0,
        emulated_time: 0,
    };
    assert_eq!(
        encode_gb_serial_frame(&gb),
        Err(LinkCableWireCodecError::InvalidGbSerialControl)
    );
}

#[test]
fn maximum_v1_integer_values_round_trip_without_unsigned_reinterpretation() {
    let frame = GbaSioMultiFrame {
        header: LinkCableWireHeader {
            room_epoch: i64::MAX as u64,
            session_epoch: i64::MAX as u64,
            cable_epoch: i64::MAX as u64,
            sender_sequence: i64::MAX as u64,
            sender_slot: 1,
        },
        event: GbaSioMultiEvent::TransferReply {
            transfer_id: u32::MAX,
            child_word: u16::MAX,
            emulated_time: i64::MAX as u64,
        },
    };
    let encoded = encode_gba_sio_multi_frame(&frame).expect("maximum v1 frame");
    assert_eq!(decode_gba_sio_multi_frame(&encoded), Ok(frame));
}

#[test]
fn gba_v2_barrier_events_use_exact_little_endian_bodies_and_v1_rejects_them() {
    let mode_ack = GbaSioMultiFrame {
        header: gba_header(9, 1),
        event: GbaSioMultiEvent::ModeAck {
            acknowledged_mode_sender_sequence: 0x0102_0304_0506_0708,
            emulated_time: 0x1112_1314_1516_1718,
        },
    };
    let encoded_mode =
        encode_gba_sio_multi_v2_frame(&mode_ack).expect("encode v2 mode acknowledgement");
    assert_eq!(encoded_mode[5], 6);
    assert_eq!(&encoded_mode[41..43], &[16, 0]);
    assert_eq!(
        &encoded_mode[43..],
        &[
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13,
            0x12, 0x11,
        ]
    );
    assert_eq!(
        decode_gba_sio_multi_v2_frame(&encoded_mode),
        Ok(mode_ack.clone())
    );
    assert_eq!(
        decode_gba_sio_multi_frame(&encoded_mode),
        Err(LinkCableWireCodecError::UnsupportedEventKind)
    );
    assert_eq!(
        encode_gba_sio_multi_frame(&mode_ack),
        Err(LinkCableWireCodecError::UnsupportedEventKind)
    );

    let finish_ack = GbaSioMultiFrame {
        header: gba_header(10, 1),
        event: GbaSioMultiEvent::FinishAck {
            transfer_id: 0x1122_3344,
            emulated_time: 0x2122_2324_2526_2728,
        },
    };
    let encoded_finish =
        encode_gba_sio_multi_v2_frame(&finish_ack).expect("encode v2 finish acknowledgement");
    assert_eq!(encoded_finish[5], 7);
    assert_eq!(&encoded_finish[41..43], &[12, 0]);
    assert_eq!(
        &encoded_finish[43..],
        &[
            0x44, 0x33, 0x22, 0x11, 0x28, 0x27, 0x26, 0x25, 0x24, 0x23, 0x22, 0x21,
        ]
    );
    assert_eq!(
        decode_link_cable_wire_frame(LinkCableWireProtocol::GbaSioMultiV2, &encoded_finish),
        Ok(LinkCableWireFrame::GbaSioMultiV2(finish_ack))
    );
    assert_eq!(
        decode_link_cable_wire_frame(LinkCableWireProtocol::GbaSioMultiV1, &encoded_finish),
        Err(LinkCableWireCodecError::UnsupportedEventKind)
    );
}

#[test]
fn protocol_and_abort_discriminators_reject_unknown_values() {
    assert_eq!(
        LinkCableWireProtocol::try_from("gba-sio-multi-v1"),
        Ok(LinkCableWireProtocol::GbaSioMultiV1)
    );
    assert_eq!(
        LinkCableWireProtocol::try_from("gba-sio-multi-v2"),
        Ok(LinkCableWireProtocol::GbaSioMultiV2)
    );
    assert_eq!(
        LinkCableWireProtocol::try_from("gb-serial-v1"),
        Ok(LinkCableWireProtocol::GbSerialV1)
    );
    assert_eq!(
        LinkCableWireProtocol::try_from("gba"),
        Err(LinkCableWireCodecError::UnsupportedProtocol)
    );
    assert_eq!(
        LinkCableAbortReason::try_from(1),
        Ok(LinkCableAbortReason::Timeout)
    );
    assert_eq!(
        LinkCableAbortReason::try_from(5),
        Ok(LinkCableAbortReason::CoreClosed)
    );
    assert_eq!(
        LinkCableAbortReason::try_from(0),
        Err(LinkCableWireCodecError::InvalidAbortReason)
    );
}

fn assert_gba_mutation(
    canonical: &[u8],
    expected: LinkCableWireCodecError,
    mutation: impl FnOnce(&mut [u8]),
) {
    let mut malformed = canonical.to_vec();
    mutation(&mut malformed);
    assert_eq!(decode_gba_sio_multi_frame(&malformed), Err(expected));
}

fn assert_gb_mutation(
    canonical: &[u8],
    expected: LinkCableWireCodecError,
    mutation: impl FnOnce(&mut [u8]),
) {
    let mut malformed = canonical.to_vec();
    mutation(&mut malformed);
    assert_eq!(decode_gb_serial_frame(&malformed), Err(expected));
}

fn zero_transfer_id(bytes: &mut [u8]) {
    bytes[LINK_CABLE_WIRE_HEADER_BYTES..LINK_CABLE_WIRE_HEADER_BYTES + 4].fill(0);
}

fn fixture_hex(file: &str) -> Vec<u8> {
    decode_hex(match file {
        "gba-mode-set.hex" => include_str!("../../../protocol/link-cable-v1/gba-mode-set.hex"),
        "gba-transfer-start.hex" => {
            include_str!("../../../protocol/link-cable-v1/gba-transfer-start.hex")
        }
        "gba-transfer-reply.hex" => {
            include_str!("../../../protocol/link-cable-v1/gba-transfer-reply.hex")
        }
        "gba-transfer-commit.hex" => {
            include_str!("../../../protocol/link-cable-v1/gba-transfer-commit.hex")
        }
        "gba-transfer-abort.hex" => {
            include_str!("../../../protocol/link-cable-v1/gba-transfer-abort.hex")
        }
        "gb-serial-start-slot0.hex" => {
            include_str!("../../../protocol/link-cable-v1/gb-serial-start-slot0.hex")
        }
        "gb-serial-start-slot1.hex" => {
            include_str!("../../../protocol/link-cable-v1/gb-serial-start-slot1.hex")
        }
        "gb-serial-reply.hex" => {
            include_str!("../../../protocol/link-cable-v1/gb-serial-reply.hex")
        }
        "gb-serial-commit.hex" => {
            include_str!("../../../protocol/link-cable-v1/gb-serial-commit.hex")
        }
        "gb-serial-abort.hex" => {
            include_str!("../../../protocol/link-cable-v1/gb-serial-abort.hex")
        }
        _ => panic!("unknown fixture file: {file}"),
    })
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .split_ascii_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).expect("fixture hex byte"))
        .collect()
}

fn fixture_case(id: &str) -> FixtureCase {
    fixture_cases()
        .into_iter()
        .find(|fixture| fixture.id == id)
        .unwrap_or_else(|| panic!("unknown fixture: {id}"))
}

fn v2_fixture_hex(file: &str) -> Vec<u8> {
    decode_hex(match file {
        "gba-mode-set.hex" => {
            include_str!("../../../protocol/link-cable-v2/gba-mode-set.hex")
        }
        "gba-transfer-start.hex" => {
            include_str!("../../../protocol/link-cable-v2/gba-transfer-start.hex")
        }
        "gba-transfer-reply.hex" => {
            include_str!("../../../protocol/link-cable-v2/gba-transfer-reply.hex")
        }
        "gba-transfer-commit.hex" => {
            include_str!("../../../protocol/link-cable-v2/gba-transfer-commit.hex")
        }
        "gba-transfer-abort.hex" => {
            include_str!("../../../protocol/link-cable-v2/gba-transfer-abort.hex")
        }
        "gba-mode-ack.hex" => {
            include_str!("../../../protocol/link-cable-v2/gba-mode-ack.hex")
        }
        "gba-finish-ack.hex" => {
            include_str!("../../../protocol/link-cable-v2/gba-finish-ack.hex")
        }
        _ => panic!("unknown GBA v2 fixture file: {file}"),
    })
}

fn gba_v2_fixture_cases() -> Vec<GbaV2FixtureCase> {
    const GBA_TIME: u64 = 0x0102_0304_0506_0708;
    let mut cases = fixture_cases()
        .into_iter()
        .filter_map(|fixture| match fixture.frame {
            LinkCableWireFrame::GbaSioMulti(frame) => Some(GbaV2FixtureCase {
                id: fixture.id,
                file: fixture.file,
                frame,
            }),
            LinkCableWireFrame::GbaSioMultiV2(_) | LinkCableWireFrame::GbSerial(_) => None,
        })
        .collect::<Vec<_>>();
    cases.extend([
        GbaV2FixtureCase {
            id: "gba-mode-ack",
            file: "gba-mode-ack.hex",
            frame: GbaSioMultiFrame {
                header: gba_header(6, 0),
                event: GbaSioMultiEvent::ModeAck {
                    acknowledged_mode_sender_sequence: 5,
                    emulated_time: GBA_TIME,
                },
            },
        },
        GbaV2FixtureCase {
            id: "gba-finish-ack",
            file: "gba-finish-ack.hex",
            frame: GbaSioMultiFrame {
                header: gba_header(6, 1),
                event: GbaSioMultiEvent::FinishAck {
                    transfer_id: 0x1122_3344,
                    emulated_time: GBA_TIME,
                },
            },
        },
    ]);
    cases
}

fn fixture_cases() -> Vec<FixtureCase> {
    const GBA_TIME: u64 = 0x0102_0304_0506_0708;
    const GB_ROOM_EPOCH: u64 = 0x0102_0304_0506_0708;
    const GB_SESSION_EPOCH: u64 = 0x1112_1314_1516_1718;
    const GB_CABLE_EPOCH: u64 = 0x2122_2324_2526_2728;

    vec![
        FixtureCase {
            id: "gba-mode-set",
            file: "gba-mode-set.hex",
            protocol: LinkCableWireProtocol::GbaSioMultiV1,
            frame: GbaSioMultiFrame {
                header: gba_header(5, 1),
                event: GbaSioMultiEvent::ModeSet {
                    mode: 2,
                    siocnt: 0x2003,
                    rcnt: 0,
                    emulated_time: GBA_TIME,
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gba-transfer-start",
            file: "gba-transfer-start.hex",
            protocol: LinkCableWireProtocol::GbaSioMultiV1,
            frame: GbaSioMultiFrame {
                header: LinkCableWireHeader {
                    room_epoch: GBA_TIME,
                    session_epoch: 9,
                    cable_epoch: 10,
                    sender_sequence: 11,
                    sender_slot: 0,
                },
                event: GbaSioMultiEvent::TransferStart {
                    transfer_id: 0x1122_3344,
                    siocnt: 0x6001,
                    parent_word: 0x99aa,
                    emulated_time: GBA_TIME,
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gba-transfer-reply",
            file: "gba-transfer-reply.hex",
            protocol: LinkCableWireProtocol::GbaSioMultiV1,
            frame: GbaSioMultiFrame {
                header: gba_header(5, 1),
                event: GbaSioMultiEvent::TransferReply {
                    transfer_id: 0x1122_3344,
                    child_word: 0x99aa,
                    emulated_time: GBA_TIME,
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gba-transfer-commit",
            file: "gba-transfer-commit.hex",
            protocol: LinkCableWireProtocol::GbaSioMultiV1,
            frame: GbaSioMultiFrame {
                header: gba_header(5, 0),
                event: GbaSioMultiEvent::TransferCommit {
                    transfer_id: 0x1122_3344,
                    words: [0xcafe, 0xbeef, 0xffff, 0xffff],
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gba-transfer-abort",
            file: "gba-transfer-abort.hex",
            protocol: LinkCableWireProtocol::GbaSioMultiV1,
            frame: GbaSioMultiFrame {
                header: gba_header(5, 1),
                event: GbaSioMultiEvent::TransferAbort {
                    transfer_id: 0x1122_3344,
                    reason: LinkCableAbortReason::PeerDisconnected,
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gb-serial-start-slot0",
            file: "gb-serial-start-slot0.hex",
            protocol: LinkCableWireProtocol::GbSerialV1,
            frame: GbSerialFrame {
                header: gb_header(
                    GB_ROOM_EPOCH,
                    GB_SESSION_EPOCH,
                    GB_CABLE_EPOCH,
                    0x3132_3334_3536_3738,
                    0,
                ),
                event: GbSerialEvent::Start {
                    transfer_id: 0x1122_3344,
                    clock_owner_slot: 0,
                    sc_control: GB_SERIAL_FAST_CLOCK_CONTROL,
                    owner_byte: 0xa5,
                    emulated_time: 0x0a1b_2c3d_4e5f_6071,
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gb-serial-start-slot1",
            file: "gb-serial-start-slot1.hex",
            protocol: LinkCableWireProtocol::GbSerialV1,
            frame: GbSerialFrame {
                header: gb_header(
                    GB_ROOM_EPOCH,
                    GB_SESSION_EPOCH,
                    GB_CABLE_EPOCH,
                    0x4142_4344_4546_4748,
                    1,
                ),
                event: GbSerialEvent::Start {
                    transfer_id: 0x5566_7788,
                    clock_owner_slot: 1,
                    sc_control: GB_SERIAL_NORMAL_CLOCK_CONTROL,
                    owner_byte: 0x5a,
                    emulated_time: 0x1a2b_3c4d_5e6f_7081,
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gb-serial-reply",
            file: "gb-serial-reply.hex",
            protocol: LinkCableWireProtocol::GbSerialV1,
            frame: GbSerialFrame {
                header: gb_header(
                    GB_ROOM_EPOCH,
                    GB_SESSION_EPOCH,
                    GB_CABLE_EPOCH,
                    0x5152_5354_5556_5758,
                    1,
                ),
                event: GbSerialEvent::Reply {
                    transfer_id: 0x1122_3344,
                    clock_owner_slot: 0,
                    responder_byte: 0x3c,
                    emulated_time: 0x2a3b_4c5d_6e7f_1021,
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gb-serial-commit",
            file: "gb-serial-commit.hex",
            protocol: LinkCableWireProtocol::GbSerialV1,
            frame: GbSerialFrame {
                header: gb_header(
                    GB_ROOM_EPOCH,
                    GB_SESSION_EPOCH,
                    GB_CABLE_EPOCH,
                    0x6162_6364_6566_6768,
                    0,
                ),
                event: GbSerialEvent::Commit {
                    transfer_id: 0x1122_3344,
                    clock_owner_slot: 0,
                    slot_bytes: [0xa5, 0x3c],
                },
            }
            .into(),
        },
        FixtureCase {
            id: "gb-serial-abort",
            file: "gb-serial-abort.hex",
            protocol: LinkCableWireProtocol::GbSerialV1,
            frame: GbSerialFrame {
                header: gb_header(
                    GB_ROOM_EPOCH,
                    GB_SESSION_EPOCH,
                    GB_CABLE_EPOCH,
                    0x7172_7374_7576_7778,
                    0,
                ),
                event: GbSerialEvent::Abort {
                    transfer_id: 0x5566_7788,
                    clock_owner_slot: 1,
                    reason: LinkCableAbortReason::ProtocolViolation,
                },
            }
            .into(),
        },
    ]
}

const fn gba_header(sender_sequence: u64, sender_slot: u8) -> LinkCableWireHeader {
    LinkCableWireHeader {
        room_epoch: 2,
        session_epoch: 3,
        cable_epoch: 4,
        sender_sequence,
        sender_slot,
    }
}

const fn gb_header(
    room_epoch: u64,
    session_epoch: u64,
    cable_epoch: u64,
    sender_sequence: u64,
    sender_slot: u8,
) -> LinkCableWireHeader {
    LinkCableWireHeader {
        room_epoch,
        session_epoch,
        cable_epoch,
        sender_sequence,
        sender_slot,
    }
}

struct FixtureCase {
    id: &'static str,
    file: &'static str,
    protocol: LinkCableWireProtocol,
    frame: LinkCableWireFrame,
}

struct GbaV2FixtureCase {
    id: &'static str,
    file: &'static str,
    frame: GbaSioMultiFrame,
}
