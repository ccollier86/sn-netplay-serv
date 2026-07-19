//! Protocol-v5 deterministic runtime compatibility profile.

use crate::limits::V5_RETROPAD_INPUT_BYTES;
use crate::protocol::descriptor_validation::{
    validate_id, validate_optional_sha256, validate_sha256,
};
use crate::protocol::{CompatibilityMismatch, V5_INPUT_CODEC_ID, V5_INPUT_PREDICTOR_ID};
use serde::{Deserialize, Serialize};

/// Whether a negotiated state digest can affect deterministic gameplay.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StateDigestMode {
    /// An exact mismatch may begin deterministic state recovery.
    Authoritative,
    /// Reports are compared and recorded but never cause recovery.
    Diagnostic,
    /// Clients do not produce state digests for this profile.
    Disabled,
}

/// Protocol-v5 deterministic runtime contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeterminismProfileV5 {
    /// Curated cross-platform core revision and patch-set identity.
    pub netplay_core_compatibility_id: String,
    /// Local binary identity used only for diagnostics, never peer equality.
    pub local_artifact_id: String,
    /// Certified deterministic platform cohort.
    pub platform_class: String,
    /// Digest of all deterministic core options.
    pub core_options_digest: String,
    /// Ordered controller port/device profile IDs.
    pub controller_profile_ids: Vec<String>,
    /// Fixed gameplay input payload codec.
    pub input_codec_id: String,
    /// Exact bytes in each fixed gameplay input payload.
    pub input_payload_size: u16,
    /// Prediction and resimulation semantics ID.
    pub predictor_id: String,
    /// Canonical nominal frame-rate numerator.
    pub nominal_frame_rate_numerator: u64,
    /// Canonical nominal frame-rate denominator.
    pub nominal_frame_rate_denominator: u64,
    /// Exact ROM/disc byte length.
    pub rom_size_bytes: u64,
    /// SHA-256 of applied patches/content transforms, or absent when unused.
    pub content_transformation_digest: Option<String>,
    /// Core-specific bootstrap and start-state loading policy.
    pub startup_state_policy_id: String,
    /// Runtime proves replay frames suppress both audio and video output.
    pub replay_output_suppressed: bool,
    /// Negotiated state-digest authority.
    pub digest_mode: StateDigestMode,
    /// Digest semantics/version ID; absent only when digest mode is disabled.
    pub digest_algorithm_id: Option<String>,
}

impl DeterminismProfileV5 {
    /// Validates the frozen v5 codec and bounded protocol identities.
    pub fn is_valid(&self) -> bool {
        validate_id(
            "determinismV5.netplayCoreCompatibilityId",
            &self.netplay_core_compatibility_id,
        )
        .is_ok()
            && validate_id("determinismV5.localArtifactId", &self.local_artifact_id).is_ok()
            && validate_id("determinismV5.platformClass", &self.platform_class).is_ok()
            && validate_sha256("determinismV5.coreOptionsDigest", &self.core_options_digest).is_ok()
            && !self.controller_profile_ids.is_empty()
            && self.controller_profile_ids.len() <= 8
            && self
                .controller_profile_ids
                .iter()
                .all(|profile| validate_id("determinismV5.controllerProfileIds", profile).is_ok())
            && self.input_codec_id == V5_INPUT_CODEC_ID
            && usize::from(self.input_payload_size) == V5_RETROPAD_INPUT_BYTES
            && self.predictor_id == V5_INPUT_PREDICTOR_ID
            && self.nominal_frame_rate_numerator > 0
            && self.nominal_frame_rate_denominator > 0
            && greatest_common_divisor(
                self.nominal_frame_rate_numerator,
                self.nominal_frame_rate_denominator,
            ) == 1
            && self.rom_size_bytes > 0
            && validate_optional_sha256(
                "determinismV5.contentTransformationDigest",
                self.content_transformation_digest.as_deref(),
            )
            .is_ok()
            && validate_id(
                "determinismV5.startupStatePolicyId",
                &self.startup_state_policy_id,
            )
            .is_ok()
            && self.replay_output_suppressed
            && self.digest_contract_is_valid()
    }

    pub(crate) fn first_mismatch(&self, other: &Self) -> Option<CompatibilityMismatch> {
        if self.netplay_core_compatibility_id != other.netplay_core_compatibility_id {
            return Some(CompatibilityMismatch::NetplayCoreCompatibility);
        }
        if self.platform_class != other.platform_class {
            return Some(CompatibilityMismatch::PlatformClass);
        }
        if !digest_matches(&self.core_options_digest, &other.core_options_digest) {
            return Some(CompatibilityMismatch::CoreOptions);
        }
        if self.controller_profile_ids != other.controller_profile_ids {
            return Some(CompatibilityMismatch::ControllerProfile);
        }
        if self.input_codec_id != other.input_codec_id
            || self.input_payload_size != other.input_payload_size
            || self.predictor_id != other.predictor_id
        {
            return Some(CompatibilityMismatch::InputContract);
        }
        if self.nominal_frame_rate_numerator != other.nominal_frame_rate_numerator
            || self.nominal_frame_rate_denominator != other.nominal_frame_rate_denominator
        {
            return Some(CompatibilityMismatch::NominalFrameRate);
        }
        if self.rom_size_bytes != other.rom_size_bytes {
            return Some(CompatibilityMismatch::RomSize);
        }
        if !optional_digest_matches(
            self.content_transformation_digest.as_deref(),
            other.content_transformation_digest.as_deref(),
        ) {
            return Some(CompatibilityMismatch::ContentTransformation);
        }
        if self.startup_state_policy_id != other.startup_state_policy_id {
            return Some(CompatibilityMismatch::StartupStatePolicy);
        }
        if self.replay_output_suppressed != other.replay_output_suppressed {
            return Some(CompatibilityMismatch::ReplayOutputSuppression);
        }
        if self.digest_mode != other.digest_mode
            || self.digest_algorithm_id != other.digest_algorithm_id
        {
            return Some(CompatibilityMismatch::DigestContract);
        }
        None
    }

    fn digest_contract_is_valid(&self) -> bool {
        match self.digest_mode {
            StateDigestMode::Disabled => self.digest_algorithm_id.is_none(),
            StateDigestMode::Authoritative | StateDigestMode::Diagnostic => self
                .digest_algorithm_id
                .as_deref()
                .is_some_and(|algorithm| {
                    validate_id("determinismV5.digestAlgorithmId", algorithm).is_ok()
                }),
        }
    }
}

fn digest_matches(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn optional_digest_matches(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => digest_matches(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}
