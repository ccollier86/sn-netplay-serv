use crate::auth::VerifiedLicense;
use crate::protocol::{
    CompatibilityFingerprint, DeterminismProfileV5, NetplaySessionDescriptor, StateDigestMode,
    StrictInputBatch, V5_INPUT_CODEC_ID, V5_INPUT_PREDICTOR_ID,
};
use crate::rooms::{
    ClientTransportCapabilities, ConnectionId, InviteCode, NetplayRoom, PlayerIndex, RoomStatus,
};
use std::time::Instant;

pub(super) struct V5RoomFixture {
    pub room: NetplayRoom,
    pub host_control: ConnectionId,
    pub guest_control: ConnectionId,
    pub host_input: ConnectionId,
    pub guest_input: ConnectionId,
}

pub(super) fn v5_room(status: RoomStatus) -> V5RoomFixture {
    let now = Instant::now();
    let host_control = ConnectionId::new();
    let guest_control = ConnectionId::new();
    let host_input = ConnectionId::new();
    let guest_input = ConnectionId::new();
    let mut room = NetplayRoom::new_with_protocol_and_resume(
        license("host"),
        host_control,
        InviteCode::parse("AB23-CD").expect("invite"),
        descriptor(),
        5,
        "host-resume".to_string(),
        "host-input".to_string(),
        now,
    );
    room.join_guest_with_resume(
        license("guest"),
        guest_control,
        "guest-resume".to_string(),
        "guest-input".to_string(),
        now,
        ClientTransportCapabilities::default(),
    )
    .expect("guest");
    room.attach_input_socket(
        PlayerIndex::ONE,
        room.room_epoch,
        room.session_epoch,
        "host-input",
        host_input,
        now,
    )
    .expect("host input");
    room.attach_input_socket(
        PlayerIndex::TWO,
        room.room_epoch,
        room.session_epoch,
        "guest-input",
        guest_input,
        now,
    )
    .expect("guest input");
    room.status = status;

    V5RoomFixture {
        room,
        host_control,
        guest_control,
        host_input,
        guest_input,
    }
}

pub(super) fn fingerprint(
    digest_mode: StateDigestMode,
    artifact_id: &str,
) -> CompatibilityFingerprint {
    let digest_algorithm_id = (digest_mode != StateDigestMode::Disabled)
        .then(|| "sha256-libretro-serialize-start-frame-v1".to_string());
    CompatibilityFingerprint {
        desktop_version: "android-2.1.0".to_string(),
        protocol_version: 5,
        system_id: "snes".to_string(),
        core_id: "snes9x".to_string(),
        core_build: artifact_id.to_string(),
        state_format: Some("snes9x:snes:libretro-serialize-v1".to_string()),
        content_hash: "a".repeat(64),
        settings_hash: empty_sha256(),
        cheats_hash: empty_sha256(),
        system_data_hash: None,
        save_data_mode: "netplay".to_string(),
        determinism_v5: Some(DeterminismProfileV5 {
            netplay_core_compatibility_id: "snes9x-2025-compat-v1".to_string(),
            local_artifact_id: artifact_id.to_string(),
            platform_class: "libretro-arm64-le-v1".to_string(),
            core_options_digest: empty_sha256(),
            controller_profile_ids: vec!["retropad-port-1-v1".to_string()],
            input_codec_id: V5_INPUT_CODEC_ID.to_string(),
            input_payload_size: 10,
            predictor_id: V5_INPUT_PREDICTOR_ID.to_string(),
            nominal_frame_rate_numerator: 150_247,
            nominal_frame_rate_denominator: 2_500,
            rom_size_bytes: 1_024,
            content_transformation_digest: None,
            startup_state_policy_id: "load-start-frame-state-v1".to_string(),
            replay_output_suppressed: true,
            digest_mode,
            digest_algorithm_id,
        }),
    }
}

fn empty_sha256() -> String {
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()
}

pub(super) fn batch(
    fixture: &V5RoomFixture,
    player_index: PlayerIndex,
    start_frame: u64,
    fills: &[u8],
) -> StrictInputBatch {
    StrictInputBatch {
        room_epoch: fixture.room.room_epoch,
        session_epoch: fixture.room.session_epoch,
        player_index,
        start_frame,
        payloads: fills.iter().map(|fill| [*fill; 10]).collect(),
    }
}

fn license(subject: &str) -> VerifiedLicense {
    VerifiedLicense::new(subject, "premium", vec!["netplay".to_string()])
}

fn descriptor() -> NetplaySessionDescriptor {
    serde_json::from_value(serde_json::json!({
        "hostClientKind": "android",
        "hostAppVersion": "2.1.0",
        "game": {
            "systemId": "snes",
            "title": "V5 Fixture",
            "romSha256": "a".repeat(64),
            "contentKey": "snes-v5-fixture"
        },
        "core": {
            "coreId": "snes9x",
            "coreOptionsSha256": empty_sha256(),
            "stateFormat": "snes9x:snes:libretro-serialize-v1"
        },
        "controller": { "inputDelayFrames": 3 },
        "romIdentity": {
            "system": "snes",
            "coreId": "snes9x",
            "contentHash": "a".repeat(64),
            "sizeBytes": 1024,
            "displayName": "V5 Fixture"
        }
    }))
    .expect("descriptor")
}
