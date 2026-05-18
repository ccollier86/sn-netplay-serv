//! Compatibility fingerprint values used before a room starts.
//!
//! Fingerprints are compared before gameplay so clients do not try to netplay
//! with different ROMs, cores, state formats, settings, cheats, or system data.

use serde::{Deserialize, Serialize};

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

        None
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
        }
    }
}
