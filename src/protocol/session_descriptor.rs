//! Room session descriptor shared before Desktop joins gameplay.
//!
//! The descriptor lets the invited client preview the game and find a matching
//! local ROM/core. It deliberately stores hashes and stable ids, never ROM data
//! or local filesystem paths.

use serde::{Deserialize, Serialize};

const ID_MAX_LEN: usize = 96;
const TITLE_MAX_LEN: usize = 160;
const SHORT_TEXT_MAX_LEN: usize = 96;
const SHA256_HEX_LEN: usize = 64;

/// Netplay game/core descriptor supplied by the host when creating a room.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplaySessionDescriptor {
    /// ShadowBoy Desktop version that created the room.
    #[serde(default)]
    pub host_app_version: Option<String>,
    /// Game identity used by the invited Desktop client to find a local ROM.
    pub game: NetplayGameDescriptor,
    /// Emulator core identity used for compatibility gating.
    pub core: NetplayCoreDescriptor,
}

impl NetplaySessionDescriptor {
    /// Validates every user/client-supplied descriptor field.
    pub fn validate(&self) -> Result<(), SessionDescriptorError> {
        validate_optional_short_text("hostAppVersion", self.host_app_version.as_deref())?;
        self.game.validate()?;
        self.core.validate()
    }
}

/// Game identity for local ROM matching.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplayGameDescriptor {
    /// Stable ShadowBoy system id, such as `gamecube` or `n64`.
    pub system_id: String,
    /// User-facing game title for invite preview.
    pub title: String,
    /// Exact ROM/content SHA-256 required for deterministic sync.
    pub rom_sha256: String,
    /// Stable library/content key that is not a local path.
    pub content_key: String,
    /// Optional region label, such as `USA`.
    #[serde(default)]
    pub region: Option<String>,
    /// Optional revision/build label.
    #[serde(default)]
    pub revision: Option<String>,
    /// Optional disc id for disc-based systems.
    #[serde(default)]
    pub disc_id: Option<String>,
}

impl NetplayGameDescriptor {
    fn validate(&self) -> Result<(), SessionDescriptorError> {
        validate_id("systemId", &self.system_id)?;
        validate_title(&self.title)?;
        validate_sha256("romSha256", &self.rom_sha256)?;
        validate_id("contentKey", &self.content_key)?;
        validate_optional_id("region", self.region.as_deref())?;
        validate_optional_id("revision", self.revision.as_deref())?;
        validate_optional_id("discId", self.disc_id.as_deref())
    }
}

/// Emulator core identity for compatibility checks.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplayCoreDescriptor {
    /// Stable ShadowBoy core id, such as `dolphin` or `mupen64plus-next`.
    pub core_id: String,
    /// Optional display name for invite preview.
    #[serde(default)]
    pub core_name: Option<String>,
    /// Optional build/version string.
    #[serde(default)]
    pub core_version: Option<String>,
    /// SHA-256 of deterministic core options when Desktop has one.
    #[serde(default)]
    pub core_options_sha256: Option<String>,
}

impl NetplayCoreDescriptor {
    fn validate(&self) -> Result<(), SessionDescriptorError> {
        validate_id("coreId", &self.core_id)?;
        validate_optional_short_text("coreName", self.core_name.as_deref())?;
        validate_optional_short_text("coreVersion", self.core_version.as_deref())?;
        validate_optional_sha256("coreOptionsSha256", self.core_options_sha256.as_deref())
    }
}

/// Descriptor validation failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid netplay session descriptor field {field}")]
pub struct SessionDescriptorError {
    /// Invalid field name in camelCase JSON form.
    pub field: &'static str,
}

fn validate_id(field: &'static str, value: &str) -> Result<(), SessionDescriptorError> {
    validate_text(field, value, ID_MAX_LEN)?;

    if value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || looks_like_windows_drive_path(value)
    {
        return Err(SessionDescriptorError { field });
    }

    Ok(())
}

fn validate_optional_id(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), SessionDescriptorError> {
    match value {
        Some(value) => validate_id(field, value),
        None => Ok(()),
    }
}

fn validate_title(value: &str) -> Result<(), SessionDescriptorError> {
    validate_text("title", value, TITLE_MAX_LEN)
}

fn validate_optional_short_text(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), SessionDescriptorError> {
    match value {
        Some(value) => validate_text(field, value, SHORT_TEXT_MAX_LEN),
        None => Ok(()),
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<(), SessionDescriptorError> {
    if value.trim().is_empty()
        || value.len() > max_len
        || value != value.trim()
        || value.chars().any(char::is_control)
    {
        Err(SessionDescriptorError { field })
    } else {
        Ok(())
    }
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), SessionDescriptorError> {
    if value.len() == SHA256_HEX_LEN && value.chars().all(|candidate| candidate.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(SessionDescriptorError { field })
    }
}

fn validate_optional_sha256(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), SessionDescriptorError> {
    match value {
        Some(value) => validate_sha256(field, value),
        None => Ok(()),
    }
}

fn looks_like_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();

    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::{NetplaySessionDescriptor, SessionDescriptorError};

    #[test]
    fn accepts_valid_descriptor() {
        assert!(descriptor().validate().is_ok());
    }

    #[test]
    fn rejects_invalid_rom_hash() {
        let mut descriptor = descriptor();
        descriptor.game.rom_sha256 = "not-a-hash".to_string();

        assert_eq!(
            descriptor.validate(),
            Err(SessionDescriptorError { field: "romSha256" })
        );
    }

    #[test]
    fn rejects_path_like_content_key() {
        let mut descriptor = descriptor();
        descriptor.game.content_key = "/home/user/game.iso".to_string();

        assert_eq!(
            descriptor.validate(),
            Err(SessionDescriptorError {
                field: "contentKey"
            })
        );
    }

    pub fn descriptor() -> NetplaySessionDescriptor {
        serde_json::from_value(serde_json::json!({
            "hostAppVersion": "0.3.0",
            "game": {
                "systemId": "gamecube",
                "title": "Star Fox Adventures",
                "romSha256": "a".repeat(64),
                "contentKey": "gamecube-star-fox-adventures-usa",
                "region": "USA",
                "revision": "Rev 1",
                "discId": "GFSE01"
            },
            "core": {
                "coreId": "dolphin",
                "coreName": "Dolphin",
                "coreVersion": "5.0-netplay",
                "coreOptionsSha256": "b".repeat(64)
            }
        }))
        .expect("descriptor")
    }
}
