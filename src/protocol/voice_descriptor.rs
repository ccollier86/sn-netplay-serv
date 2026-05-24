//! Optional voice-chat descriptor supplied at netplay room creation.
//!
//! These values are shared with clients as room setup intent. LiveKit tokens
//! and provider-specific secrets stay outside this descriptor.

use serde::{Deserialize, Serialize};

/// Voice-chat request attached to a netplay session descriptor.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetplayVoiceDescriptor {
    /// Whether the host wants a voice room for this multiplayer session.
    #[serde(default)]
    pub enabled: bool,
    /// Initial microphone behavior for players that join the voice room.
    #[serde(default)]
    pub mode: NetplayVoiceMode,
}

/// Initial voice behavior selected by the host.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetplayVoiceMode {
    /// Mic opens automatically when the client-side threshold is crossed.
    #[default]
    VoiceActivation,
    /// Mic opens only while the player holds the push-to-talk shortcut.
    PushToTalk,
    /// Player joins muted and can opt in from the client UI.
    MutedOnJoin,
}
