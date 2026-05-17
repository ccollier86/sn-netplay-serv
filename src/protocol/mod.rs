//! Serializable netplay protocol value types.
//!
//! Protocol modules define wire-compatible messages and validation helpers.
//! They do not own room storage or transport socket lifetimes.

mod client_message;
mod compatibility;
mod input_frame;
mod netplay_protocol;
mod server_message;
mod session_descriptor;
mod snapshot;

pub use client_message::ClientMessage;
pub use compatibility::{CompatibilityFingerprint, CompatibilityMismatch};
pub use input_frame::{InputFrame, InputFrameLimits};
pub use netplay_protocol::{
    MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION, NETPLAY_PROTOCOL_VERSION, NetplayProtocolView,
    ProtocolVersionError, validate_client_protocol_version,
};
pub use server_message::ServerMessage;
pub use session_descriptor::{
    NetplayCoreDescriptor, NetplayGameDescriptor, NetplaySessionDescriptor, SessionDescriptorError,
};
pub use snapshot::{SnapshotChunk, SnapshotLimits, SnapshotManifest};
