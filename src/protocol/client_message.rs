//! Client-to-server WebSocket messages.
//!
//! These message types are transport payloads only. Domain validation still
//! happens in room modules before input or state is accepted.

use crate::protocol::{
    CompatibilityFingerprint, InputFrame, LinkCableCompatibility, LinkCablePacket,
    SessionPauseReason, SnapshotChunk, SnapshotManifest,
};
use serde::Deserialize;

/// Message sent by a Desktop client over a room WebSocket.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    /// Lightweight connection keepalive.
    Ping,
    /// Client compatibility fingerprint for the current game/core.
    SetCompatibilityFingerprint {
        /// Netplay-relevant compatibility details.
        fingerprint: CompatibilityFingerprint,
    },
    /// Client link-cable compatibility for the selected runtime.
    SetLinkCableCompatibility {
        /// Link-cable runtime compatibility details.
        compatibility: LinkCableCompatibility,
    },
    /// Client is ready to start or continue the sync phase.
    Ready,
    /// One chunk of host save-state snapshot data.
    SnapshotChunk {
        /// Chunk payload.
        chunk: SnapshotChunk,
    },
    /// Manifest for a completed snapshot transfer.
    SnapshotComplete {
        /// Snapshot manifest.
        manifest: SnapshotManifest,
    },
    /// Frame-numbered input from the local player.
    InputFrame {
        /// Normalized input payload.
        input: InputFrame,
    },
    /// Opaque virtual link-cable packet from the local runtime.
    LinkCablePacket {
        /// Link packet to relay.
        packet: LinkCablePacket,
    },
    /// Request a room-wide pause at a relay-selected frame.
    RequestSessionPause {
        /// Client-generated id for logs and idempotent UI actions.
        request_id: String,
        /// Reason the client is pausing.
        reason: SessionPauseReason,
        /// Client's local frame when it requested the pause.
        local_frame: u64,
    },
    /// A client reached and paused at the scheduled frame.
    SessionPauseReached {
        /// Pause sequence being acknowledged.
        sequence: u64,
        /// Frame where the runtime actually paused.
        paused_at_frame: u64,
    },
    /// Release this client's pause holder and resume if every holder is gone.
    RequestSessionResume {
        /// Client-generated id for logs and idempotent UI actions.
        request_id: String,
        /// Pause sequence being released.
        sequence: u64,
        /// Reason being released.
        reason: SessionPauseReason,
    },
}
