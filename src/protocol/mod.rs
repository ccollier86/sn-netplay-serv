//! Serializable netplay protocol value types.
//!
//! Protocol modules define wire-compatible messages and validation helpers.
//! They do not own room storage or transport socket lifetimes.

mod client_message;
mod compatibility;
mod input_frame;
mod server_message;
mod snapshot;

pub use client_message::ClientMessage;
pub use compatibility::{CompatibilityFingerprint, CompatibilityMismatch};
pub use input_frame::{InputFrame, InputFrameLimits};
pub use server_message::ServerMessage;
pub use snapshot::{SnapshotChunk, SnapshotLimits, SnapshotManifest};
