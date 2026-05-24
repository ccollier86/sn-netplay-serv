//! JSON DTOs used by the trusted voice broker.
//!
//! These mirror `sb-webrtc` without creating a compile-time dependency between
//! the relay server and the standalone voice service.

use crate::protocol::NetplayVoiceMode;
use serde::{Deserialize, Serialize};

/// Voice room creation request sent to the broker.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVoiceRoomBrokerRequest {
    /// Netplay room id that owns this voice room.
    pub netplay_room_id: String,
    /// Human invite code for diagnostics.
    pub netplay_invite_code: String,
    /// Netplay room epoch at voice creation time.
    pub room_epoch: u64,
    /// Host-selected voice behavior.
    pub mode: NetplayVoiceMode,
    /// Maximum participants expected in the voice room.
    pub max_participants: u8,
    /// Player slots requiring private voice tokens.
    pub participants: Vec<VoiceBrokerParticipant>,
}

/// Participant token target for a voice room.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceBrokerParticipant {
    /// One-based ShadowBoy player slot.
    pub player_index: u8,
    /// Stable LiveKit identity.
    pub participant_identity: String,
    /// Display name shown by clients.
    pub display_name: String,
}

/// Voice room creation response returned by the broker.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVoiceRoomBrokerResponse {
    /// Shared voice room metadata.
    pub room: VoiceBrokerRoomView,
    /// Player-specific join grants.
    pub grants: Vec<VoiceBrokerGrant>,
}

/// Shared voice room metadata returned by the broker.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceBrokerRoomView {
    /// ShadowBoy voice broker room id.
    pub voice_room_id: String,
    /// LiveKit room name.
    pub livekit_room_name: String,
    /// Public LiveKit WebSocket URL.
    pub server_url: String,
    /// Netplay room id for correlation.
    pub netplay_room_id: String,
    /// Netplay invite code for correlation.
    pub netplay_invite_code: String,
    /// Netplay room epoch.
    pub room_epoch: u64,
    /// Host-selected voice behavior.
    pub mode: NetplayVoiceMode,
    /// Maximum participants allowed.
    pub max_participants: u8,
}

/// Player-specific join grant returned by the broker.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceBrokerGrant {
    /// One-based ShadowBoy player slot.
    pub player_index: u8,
    /// Stable LiveKit identity.
    pub participant_identity: String,
    /// Provider join token.
    pub token: String,
    /// RFC3339 expiration timestamp.
    pub expires_at: String,
}

/// Token refresh request sent to the broker.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueVoiceTokenBrokerRequest {
    /// One-based ShadowBoy player slot.
    pub player_index: u8,
    /// Stable LiveKit identity.
    pub participant_identity: String,
    /// Optional display name.
    pub display_name: Option<String>,
}

/// Voice room close request sent to the broker.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseVoiceRoomRequest {
    /// Cleanup reason for broker telemetry.
    pub reason: String,
}
