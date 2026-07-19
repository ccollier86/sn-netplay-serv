//! Compatibility fingerprint values used before a room starts.
//!
//! Fingerprints are compared before gameplay so clients do not try to netplay
//! with different ROMs, cores, state formats, settings, cheats, or system data.

use serde::{Deserialize, Serialize};

use crate::protocol::compatibility_v5::DeterminismProfileV5;
use crate::protocol::descriptor_validation::{validate_optional_sha256, validate_sha256};

/// Netplay-relevant compatibility fingerprint for one client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityFingerprint {
    /// ShadowBoy client application version.
    pub desktop_version: String,
    /// Netplay protocol version.
    pub protocol_version: u16,
    /// ShadowBoy system id.
    pub system_id: String,
    /// Emulator core id.
    pub core_id: String,
    /// Core version or build hash; informational only for compatibility.
    pub core_build: String,
    /// Save-state byte format that must be loadable by both clients.
    #[serde(default)]
    pub state_format: Option<String>,
    /// Hash of the ROM/disc content.
    pub content_hash: String,
    /// Hash of netplay-relevant emulator settings.
    pub settings_hash: String,
    /// Hash of enabled cheats or codes.
    pub cheats_hash: String,
    /// Hash of BIOS/system data if required by the core.
    pub system_data_hash: Option<String>,
    /// Save-data mode, for example `netplay`.
    pub save_data_mode: String,
    /// Required deterministic contract for protocol-v5 rooms.
    #[serde(default)]
    pub determinism_v5: Option<DeterminismProfileV5>,
}

impl CompatibilityFingerprint {
    /// Compares two fingerprints and returns the first user-meaningful mismatch.
    pub fn first_mismatch(&self, other: &Self) -> Option<CompatibilityMismatch> {
        if self.protocol_version != other.protocol_version {
            return Some(CompatibilityMismatch::ProtocolVersion);
        }
        if self.system_id != other.system_id {
            return Some(CompatibilityMismatch::System);
        }
        if self.core_id != other.core_id {
            return Some(CompatibilityMismatch::Core);
        }
        if self.normalized_state_format() != other.normalized_state_format() {
            return Some(CompatibilityMismatch::StateFormat);
        }
        if self.content_hash != other.content_hash {
            return Some(CompatibilityMismatch::Content);
        }
        if self.settings_hash != other.settings_hash {
            return Some(CompatibilityMismatch::Settings);
        }
        if self.cheats_hash != other.cheats_hash {
            return Some(CompatibilityMismatch::Cheats);
        }
        if self.system_data_hash != other.system_data_hash {
            return Some(CompatibilityMismatch::SystemData);
        }
        if self.save_data_mode != other.save_data_mode {
            return Some(CompatibilityMismatch::SaveDataMode);
        }
        if self.protocol_version >= 5 {
            let (Some(left), Some(right)) =
                (self.determinism_v5.as_ref(), other.determinism_v5.as_ref())
            else {
                return Some(CompatibilityMismatch::DeterminismProfileMissing);
            };
            if let Some(mismatch) = left.first_mismatch(right) {
                return Some(mismatch);
            }
        }

        None
    }

    /// Returns the negotiated v5 profile only when all fields are valid.
    pub fn valid_determinism_v5(&self) -> Option<&DeterminismProfileV5> {
        let hashes_are_valid = validate_sha256("contentHash", &self.content_hash).is_ok()
            && validate_sha256("settingsHash", &self.settings_hash).is_ok()
            && validate_sha256("cheatsHash", &self.cheats_hash).is_ok()
            && validate_optional_sha256("systemDataHash", self.system_data_hash.as_deref()).is_ok();

        self.determinism_v5
            .as_ref()
            .filter(|profile| hashes_are_valid && profile.is_valid())
    }

    fn normalized_state_format(&self) -> &str {
        self.state_format.as_deref().unwrap_or(&self.core_build)
    }
}

/// First mismatch reason found between two compatibility fingerprints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityMismatch {
    /// Clients speak different netplay protocol versions.
    ProtocolVersion,
    /// Clients selected different systems.
    System,
    /// Clients selected different cores.
    Core,
    /// Clients selected incompatible save-state byte formats.
    StateFormat,
    /// Clients have different ROM or disc content.
    Content,
    /// Netplay-relevant settings differ.
    Settings,
    /// Enabled cheats or codes differ.
    Cheats,
    /// Required BIOS/system data differs.
    SystemData,
    /// Save-data mode differs.
    SaveDataMode,
    /// A protocol-v5 fingerprint omitted its deterministic profile.
    DeterminismProfileMissing,
    /// Curated core revisions or deterministic patch sets differ.
    NetplayCoreCompatibility,
    /// Clients are not in the same certified platform cohort.
    PlatformClass,
    /// Deterministic core options differ.
    CoreOptions,
    /// Controller port/device contracts differ.
    ControllerProfile,
    /// Input codec, payload size, or predictor differs.
    InputContract,
    /// Canonical rational core cadence differs.
    NominalFrameRate,
    /// ROM/disc byte lengths differ.
    RomSize,
    /// Applied patch/content transform differs.
    ContentTransformation,
    /// Startup state/bootstrap behavior differs.
    StartupStatePolicy,
    /// A runtime cannot suppress replay output.
    ReplayOutputSuppression,
    /// State-digest authority or algorithm differs.
    DigestContract,
}

#[cfg(test)]
mod tests {
    use super::{CompatibilityFingerprint, CompatibilityMismatch};

    #[test]
    fn reports_first_mismatch() {
        let left = fixture("rom-a");
        let right = fixture("rom-b");

        assert_eq!(
            left.first_mismatch(&right),
            Some(CompatibilityMismatch::Content)
        );
    }

    #[test]
    fn matching_fingerprints_have_no_mismatch() {
        let left = fixture("rom-a");
        let right = fixture("rom-a");

        assert_eq!(left.first_mismatch(&right), None);
    }

    #[test]
    fn ignores_core_build_when_state_format_matches() {
        let left = fixture("rom-a");
        let mut right = fixture("rom-a");

        right.core_build = "android-build".to_string();

        assert_eq!(left.first_mismatch(&right), None);
    }

    #[test]
    fn reports_state_format_mismatch() {
        let left = fixture("rom-a");
        let mut right = fixture("rom-a");

        right.state_format = Some("different-state-format".to_string());

        assert_eq!(
            left.first_mismatch(&right),
            Some(CompatibilityMismatch::StateFormat)
        );
    }

    fn fixture(content_hash: &str) -> CompatibilityFingerprint {
        CompatibilityFingerprint {
            desktop_version: "0.2.10".to_string(),
            protocol_version: 1,
            system_id: "n64".to_string(),
            core_id: "mupen64plus-next".to_string(),
            core_build: "core-build".to_string(),
            state_format: Some("mupen64plus-next:n64:libretro-serialize-v1".to_string()),
            content_hash: content_hash.to_string(),
            settings_hash: "settings".to_string(),
            cheats_hash: "cheats".to_string(),
            system_data_hash: None,
            save_data_mode: "netplay".to_string(),
            determinism_v5: None,
        }
    }
}
