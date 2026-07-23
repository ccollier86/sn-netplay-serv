//! Capability-gated link-cable lobby wire models.
//!
//! These additive DTOs describe normal-lobby coordination for GB/GBC and GBA
//! link sessions. They do not alter controller-netplay state or carry ROM data.

use crate::lobbies::LobbyGameCandidate;
use serde::{Deserialize, Serialize};

/// Version of the normal-lobby link-cable coordination contract.
pub const LOBBY_LINK_CABLE_CONTRACT_VERSION: u16 = 1;
/// Fixed player capacity of the current mGBA link-cable provider.
pub const MAX_LINK_CABLE_LOBBY_PLAYERS: u8 = 2;

/// Frozen link protocol family selected for one lobby session.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyLinkProtocolFamily {
    /// GB/GBC two-device serial exchange.
    GbSerialV1,
    /// GBA two-device multiplayer SIO exchange.
    GbaMultiV1,
}

/// Link-cable behavior supported by one lobby client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyLinkCableClientCapabilities {
    /// Lobby link-cable contract version implemented by this client.
    pub contract_version: u16,
    /// Runtime compatibility profile used by the mGBA integration.
    pub runtime_profile: String,
    /// Exact mGBA/core build identifier used for peer compatibility.
    pub core_build_id: String,
    /// Frozen link protocol families this client can run.
    pub protocol_families: Vec<LobbyLinkProtocolFamily>,
}

/// Specialized multiplayer behavior resolved for a normal lobby.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyMultiplayerSessionKind {
    /// Existing shared-game controller netplay.
    ControllerNetplay,
    /// Per-player games connected through a virtual link cable.
    LinkCable,
    /// Future externally hosted multiplayer network.
    ExternalNetwork,
}

/// Independent launch/runtime state for one link-cable player.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LobbyLinkCableLaunchState {
    /// A compatible local game is selected but not running.
    NotLaunched,
    /// The client is starting its selected game.
    Launching,
    /// The local runtime is attached to the link-cable room.
    RuntimeAttached,
    /// The local game stopped normally.
    Stopped,
    /// The local runtime or link route was interrupted.
    Interrupted,
}

/// Per-player link selection and independent runtime state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyLinkCablePlayerSlotView {
    /// Zero-based lobby player index.
    pub player_index: u8,
    /// Zero-based virtual cable endpoint assigned to this player.
    pub cable_slot: u8,
    /// Monotonic generation for this player's selected game.
    pub selection_generation: u64,
    /// Player-local game selection. Peers may select different ROMs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_game: Option<LobbyGameCandidate>,
    /// Current independent launch/runtime state.
    pub launch_state: LobbyLinkCableLaunchState,
    /// Last state-change timestamp in milliseconds since unix epoch.
    pub updated_at_ms: u128,
}

/// Link-cable projection attached to a normal lobby.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyLinkCableView {
    /// Frozen GB/GBC or GBA wire family for this session.
    pub protocol_family: LobbyLinkProtocolFamily,
    /// Fixed two-player capacity of the current mGBA link provider.
    pub max_players: u8,
    /// Existing direct link-room invite shared only through lobby membership.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_invite_code: Option<String>,
    /// Current data-plane cable epoch when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cable_epoch: Option<u64>,
    /// Two independent player game/runtime slots.
    pub players: Vec<LobbyLinkCablePlayerSlotView>,
}

/// Additive specialized-multiplayer state for capable lobby clients.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyMultiplayerExtension {
    /// Specialized session kind resolved for this lobby generation.
    pub session_kind: LobbyMultiplayerSessionKind,
    /// Monotonic generation for the resolved multiplayer session.
    pub generation: u64,
    /// Link-cable state when `sessionKind` is `linkCable`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_cable: Option<LobbyLinkCableView>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn link_capabilities_use_frozen_camel_case_wire_values() {
        let capabilities = LobbyLinkCableClientCapabilities {
            contract_version: LOBBY_LINK_CABLE_CONTRACT_VERSION,
            runtime_profile: "mgba-link-runtime-v1".to_owned(),
            core_build_id: "android-mgba-link-v1".to_owned(),
            protocol_families: vec![
                LobbyLinkProtocolFamily::GbSerialV1,
                LobbyLinkProtocolFamily::GbaMultiV1,
            ],
        };

        let payload = serde_json::to_value(&capabilities).expect("capabilities serialize");

        assert_eq!(
            payload,
            json!({
                "contractVersion": 1,
                "runtimeProfile": "mgba-link-runtime-v1",
                "coreBuildId": "android-mgba-link-v1",
                "protocolFamilies": ["gbSerialV1", "gbaMultiV1"]
            })
        );
        assert_eq!(
            serde_json::from_value::<LobbyLinkCableClientCapabilities>(payload)
                .expect("capabilities deserialize"),
            capabilities
        );
    }

    #[test]
    fn link_extension_serializes_independent_player_state_and_fixed_capacity() {
        let extension = LobbyMultiplayerExtension {
            session_kind: LobbyMultiplayerSessionKind::LinkCable,
            generation: 3,
            link_cable: Some(LobbyLinkCableView {
                protocol_family: LobbyLinkProtocolFamily::GbaMultiV1,
                max_players: MAX_LINK_CABLE_LOBBY_PLAYERS,
                room_invite_code: Some("AB23-CD".to_owned()),
                cable_epoch: Some(5),
                players: vec![
                    player(
                        0,
                        2,
                        "Pokemon Emerald",
                        LobbyLinkCableLaunchState::RuntimeAttached,
                        100,
                    ),
                    player(
                        1,
                        4,
                        "Pokemon Sapphire",
                        LobbyLinkCableLaunchState::Interrupted,
                        120,
                    ),
                ],
            }),
        };

        let payload = serde_json::to_value(&extension).expect("extension serializes");

        assert_eq!(payload["sessionKind"], "linkCable");
        assert_eq!(payload["generation"], 3);
        assert_eq!(payload["linkCable"]["protocolFamily"], "gbaMultiV1");
        assert_eq!(payload["linkCable"]["maxPlayers"], 2);
        assert_eq!(payload["linkCable"]["roomInviteCode"], "AB23-CD");
        assert_eq!(payload["linkCable"]["cableEpoch"], 5);
        assert_eq!(
            payload["linkCable"]["players"][0]["launchState"],
            "runtimeAttached"
        );
        assert_eq!(
            payload["linkCable"]["players"][1]["launchState"],
            "interrupted"
        );
        assert_eq!(
            serde_json::from_value::<LobbyMultiplayerExtension>(payload)
                .expect("extension deserializes"),
            extension
        );
    }

    fn player(
        player_index: u8,
        selection_generation: u64,
        title: &str,
        launch_state: LobbyLinkCableLaunchState,
        updated_at_ms: u128,
    ) -> LobbyLinkCablePlayerSlotView {
        LobbyLinkCablePlayerSlotView {
            player_index,
            cable_slot: player_index,
            selection_generation,
            selected_game: Some(LobbyGameCandidate {
                title: title.to_owned(),
                system_id: "gba".to_owned(),
                core_id: "mgba".to_owned(),
                content_sha256: None,
                rom_size_bytes: None,
                start_state_label: None,
            }),
            launch_state,
            updated_at_ms,
        }
    }
}
