//! Direct-invite temporary ROM relay protocol types.
//!
//! These types are metadata-only. The file relay service owns byte movement and
//! Android/Desktop clients own local hashing, storage, and emulator paths.

use serde::{Deserialize, Serialize};

/// Direct invite room shape used by Android's in-game invite flow.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetplayRoomMode {
    /// Host creates the room from a running game and the guest joins by code.
    #[default]
    DirectInvite,
}

/// Host intent for temporary ROM relay in direct-invite rooms.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RomRelayIntent {
    /// Exact local ROM match only.
    #[default]
    ExactMatchOnly,
    /// Offer temporary relay when the peer is missing the exact ROM.
    MissingPeerOnly,
}

/// ROM identity provided by the direct-invite host.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RomIdentity {
    /// Stable system id.
    pub system: String,
    /// Stable emulator core id.
    pub core_id: String,
    /// Expected content hash, raw SHA-256 or `sha256:<digest>`.
    pub content_hash: String,
    /// Complete payload byte size.
    pub size_bytes: u64,
    /// Original file name, when safe to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    /// Original file extension, when safe to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    /// User-facing display name.
    pub display_name: String,
}

impl RomIdentity {
    /// Returns a lower-case raw SHA-256 digest without a `sha256:` prefix.
    pub fn normalized_hash(&self) -> String {
        normalize_content_hash(&self.content_hash)
    }
}

/// Safe preview of direct ROM relay availability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RomRelayCapability {
    /// Whether this room shape can ever support ROM relay.
    pub supported: bool,
    /// Whether this exact room can currently create grants.
    pub available: bool,
    /// Relay grants are session-scoped and do not imply ownership/import.
    pub temporary_access_only: bool,
    /// Maximum ROM payload bytes allowed by server policy.
    pub max_bytes: u64,
    /// Server allowlist for direct ROM relay systems.
    pub allowed_systems: Vec<String>,
    /// Machine-readable reason when `available` is false.
    pub reason: Option<RomRelayCapabilityReason>,
}

/// Reason direct ROM relay is unavailable.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RomRelayCapabilityReason {
    /// Direct ROM relay is disabled by server policy.
    Disabled,
    /// The trusted file relay broker is not configured.
    BrokerUnavailable,
    /// The room is not an Android direct-invite room.
    UnsupportedRoom,
    /// The host did not provide enough ROM identity metadata.
    MissingIdentity,
    /// The requested ROM is larger than server policy allows.
    TooLarge,
    /// The system is outside the direct relay allowlist.
    UnsupportedSystem,
}

/// Transfer grant role.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RomRelayGrantRole {
    /// Sender uploads the ROM to the file relay.
    Upload,
    /// Receiver downloads the ROM from the file relay.
    Download,
}

/// Private file relay grant for one side of a ROM transfer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RomRelayGrant {
    /// File relay transfer id.
    pub transfer_id: String,
    /// Public file relay base URL.
    pub relay_url: String,
    /// Opaque per-transfer token for this client.
    pub token: String,
    /// Upload or download role.
    pub role: RomRelayGrantRole,
    /// ROM identity this grant is scoped to.
    pub rom: RomIdentity,
    /// Zero-based sender player index.
    pub sender_player_index: u8,
    /// Zero-based receiver player index.
    pub receiver_player_index: u8,
    /// File relay chunk size.
    pub chunk_size_bytes: u64,
    /// Expected file relay chunk count.
    pub chunk_count: u64,
    /// RFC3339 grant expiry.
    pub expires_at: String,
}

/// Upload/download progress reported by a client or echoed by the server.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RomRelayProgress {
    /// File relay transfer id.
    pub transfer_id: String,
    /// Upload or download phase.
    pub role: RomRelayGrantRole,
    /// Bytes completed by the client.
    pub bytes_complete: u64,
    /// Total bytes expected.
    pub size_bytes: u64,
}

/// Completion payload for upload/download acknowledgements.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RomRelayCompletion {
    /// File relay transfer id.
    pub transfer_id: String,
    /// Upload or download phase being completed.
    pub role: RomRelayGrantRole,
    /// Hash verified by the sender/receiver.
    pub content_hash: String,
}

/// Relay failure payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RomRelayFailure {
    /// File relay transfer id, when a transfer was already created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_id: Option<String>,
    /// Stable failure reason.
    pub reason: RomRelayFailReason,
}

/// Stable ROM relay failure reasons.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RomRelayFailReason {
    /// File relay broker did not create a transfer.
    BrokerUnavailable,
    /// Upload/download hash did not match the room identity.
    HashMismatch,
    /// Client reported a transport failure.
    TransferFailed,
    /// Client sent a stale room or session epoch.
    StaleEpoch,
    /// Client payload was invalid for the active transfer.
    InvalidPayload,
}

/// Stable ROM relay block reasons.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RomRelayBlockReason {
    /// Feature flag disabled the path.
    Disabled,
    /// File relay broker is not configured.
    BrokerUnavailable,
    /// Caller is not the guest or expected player.
    WrongPlayer,
    /// Host or guest did not advertise support.
    ClientUnsupported,
    /// System is outside the direct relay allowlist.
    UnsupportedSystem,
    /// ROM is larger than server policy allows.
    TooLarge,
    /// Room has no valid ROM identity.
    MissingIdentity,
    /// Another ROM transfer is already active.
    TransferActive,
}

/// Block payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RomRelayBlocked {
    /// Stable block reason.
    pub reason: RomRelayBlockReason,
}

/// Cancel payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RomRelayCancelled {
    /// File relay transfer id, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_id: Option<String>,
}

/// Returns whether a content hash is a raw SHA-256 or `sha256:`-prefixed digest.
pub fn is_content_hash(value: &str) -> bool {
    is_sha256_hex(value.trim())
        || value
            .trim()
            .strip_prefix("sha256:")
            .is_some_and(is_sha256_hex)
}

/// Normalizes content hash values to lower-case raw SHA-256.
pub fn normalize_content_hash(value: &str) -> String {
    value
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or_else(|| value.trim())
        .to_ascii_lowercase()
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
