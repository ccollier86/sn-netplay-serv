//! Room session descriptor shared before clients join gameplay.
//!
//! The descriptor lets the invited client preview the game and find a matching
//! local ROM/core or compatible link runtime. It deliberately stores hashes and
//! stable ids, never ROM data or local filesystem paths.

use crate::protocol::descriptor_validation::{
    validate_id, validate_optional_id, validate_optional_sha256, validate_optional_short_text,
    validate_sha256, validate_title,
};
use crate::protocol::{LinkCableDescriptor, NetplaySessionMode, SessionDescriptorError};
use serde::{Deserialize, Serialize};

/// Netplay game/core descriptor supplied by the host when creating a room.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplaySessionDescriptor {
    /// ShadowBoy Desktop version that created the room.
    #[serde(default)]
    pub host_app_version: Option<String>,
    /// High-level room behavior selected by the host.
    #[serde(default)]
    pub mode: NetplaySessionMode,
    /// Game identity used by the invited Desktop client to find a local ROM.
    pub game: NetplayGameDescriptor,
    /// Emulator core identity used for compatibility gating.
    pub core: NetplayCoreDescriptor,
    /// Link-cable compatibility details for `linkCable` rooms.
    #[serde(default)]
    pub link: Option<LinkCableDescriptor>,
}

impl NetplaySessionDescriptor {
    /// Validates every user/client-supplied descriptor field.
    pub fn validate(&self) -> Result<(), SessionDescriptorError> {
        validate_optional_short_text("hostAppVersion", self.host_app_version.as_deref())?;
        self.game.validate()?;
        self.core.validate()?;
        self.validate_mode()
    }

    fn validate_mode(&self) -> Result<(), SessionDescriptorError> {
        match (self.mode, self.link.as_ref()) {
            (NetplaySessionMode::ControllerNetplay, None) => Ok(()),
            (NetplaySessionMode::ControllerNetplay, Some(_)) => {
                Err(SessionDescriptorError { field: "link" })
            }
            (NetplaySessionMode::LinkCable, Some(link)) => link.validate(),
            (NetplaySessionMode::LinkCable, None) => Err(SessionDescriptorError { field: "link" }),
        }
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

#[cfg(test)]
mod tests {
    use super::{NetplaySessionDescriptor, SessionDescriptorError};
    use crate::protocol::NetplaySessionMode;
    use serde_json::json;

    #[test]
    fn accepts_legacy_controller_descriptor() {
        assert!(descriptor().validate().is_ok());
    }

    #[test]
    fn defaults_legacy_descriptor_to_controller_mode() {
        assert_eq!(descriptor().mode, NetplaySessionMode::ControllerNetplay);
    }

    #[test]
    fn accepts_link_cable_descriptor() {
        let mut value = descriptor_value();
        value["mode"] = json!("linkCable");
        value["link"] = json!({
            "systemFamily": "gba",
            "linkProtocol": "gba-link-cable-v1",
            "runtimeProfile": "mgba-link-runtime-v1",
            "maxPlayers": 2,
            "transport": "relay"
        });
        let descriptor =
            serde_json::from_value::<NetplaySessionDescriptor>(value).expect("link descriptor");

        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn rejects_link_mode_without_link_details() {
        let mut descriptor = descriptor();
        descriptor.mode = NetplaySessionMode::LinkCable;

        assert_eq!(
            descriptor.validate(),
            Err(SessionDescriptorError { field: "link" })
        );
    }

    #[test]
    fn rejects_link_details_for_controller_mode() {
        let mut value = descriptor_value();
        value["link"] = json!({
            "systemFamily": "gba",
            "linkProtocol": "gba-link-cable-v1",
            "runtimeProfile": "mgba-link-runtime-v1",
            "maxPlayers": 2
        });
        let descriptor =
            serde_json::from_value::<NetplaySessionDescriptor>(value).expect("descriptor");

        assert_eq!(
            descriptor.validate(),
            Err(SessionDescriptorError { field: "link" })
        );
    }

    #[test]
    fn rejects_unsupported_link_player_count() {
        let mut value = descriptor_value();
        value["mode"] = json!("linkCable");
        value["link"] = json!({
            "systemFamily": "gba",
            "linkProtocol": "gba-link-cable-v1",
            "runtimeProfile": "mgba-link-runtime-v1",
            "maxPlayers": 4
        });
        let descriptor =
            serde_json::from_value::<NetplaySessionDescriptor>(value).expect("descriptor");

        assert_eq!(
            descriptor.validate(),
            Err(SessionDescriptorError {
                field: "link.maxPlayers"
            })
        );
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
        serde_json::from_value(descriptor_value()).expect("descriptor")
    }

    fn descriptor_value() -> serde_json::Value {
        serde_json::json!({
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
        })
    }
}
