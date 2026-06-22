//! Serializable netplay protocol value types.
//!
//! Protocol modules define wire-compatible messages and validation helpers.
//! They do not own room storage or transport socket lifetimes.

mod client_kind;
mod client_message;
mod client_network_quality;
mod client_runtime_state;
mod clock_sync;
mod compatibility;
mod descriptor_validation;
mod fast_input;
mod input_batch;
mod input_delay_change;
mod input_frame;
mod link_cable_compatibility;
mod link_cable_descriptor;
mod link_cable_packet;
mod lobby_file_relay;
mod lobby_messages;
mod netplay_protocol;
mod rom_relay;
mod scheduled_start;
mod server_frame;
mod server_message;
mod session_descriptor;
mod session_descriptor_error;
mod session_mode;
mod session_pause;
mod snapshot;
mod snapshot_file_relay;
mod state_hash;
mod voice_descriptor;

pub use client_kind::NetplayClientKind;
pub use client_message::ClientMessage;
pub use client_network_quality::ClientNetworkQualityReport;
pub use client_runtime_state::ClientRuntimeState;
pub use clock_sync::{
    ClockSyncEstimate, ClockSyncPing, ClockSyncPong, ClockSyncSample, ClockSyncSampleRequest,
};
pub use compatibility::{CompatibilityFingerprint, CompatibilityMismatch};
pub use fast_input::{
    FastInputBatch, FastInputCodecError, FastInputFrame, decode_fast_input_batch,
    encode_fast_input_frame,
};
pub use input_batch::{
    InputFrameBatch, InputFrameBatchCodecError, decode_input_frame_batch, encode_input_frame_batch,
};
pub use input_delay_change::{InputDelayChange, InputDelayChangeReason};
pub use input_frame::{InputFrame, InputFrameLimits};
pub use link_cable_compatibility::LinkCableCompatibility;
pub use link_cable_descriptor::{LinkCableDescriptor, LinkCableTransport};
pub use link_cable_packet::{LinkCablePacket, LinkCablePacketError, LinkCablePacketLimits};
pub use lobby_file_relay::{LobbyFileRelayGrant, LobbyFileRelayGrantPair, LobbyFileRelayGrantRole};
pub use lobby_messages::{LobbyClientMessage, LobbyServerMessage};
pub use netplay_protocol::{
    MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION, NETPLAY_PROTOCOL_VERSION, NetplayProtocolView,
    ProtocolVersionError, validate_client_protocol_version,
};
pub use rom_relay::{
    NetplayRoomMode, RomIdentity, RomRelayBlockReason, RomRelayBlocked, RomRelayCancelled,
    RomRelayCapability, RomRelayCapabilityReason, RomRelayCompletion, RomRelayFailReason,
    RomRelayFailure, RomRelayGrant, RomRelayGrantRole, RomRelayIntent, RomRelayProgress,
    is_content_hash, normalize_content_hash,
};
pub use scheduled_start::{DeterministicReadyReport, ScheduledSessionStart};
pub use server_frame::{
    ServerFrame, ServerFrameCodecError, decode_server_frame, encode_server_frame,
};
pub use server_message::ServerMessage;
pub use session_descriptor::{
    ControllerNetplayDescriptor, DEFAULT_CONTROLLER_INPUT_DELAY_FRAMES,
    MAX_CONTROLLER_INPUT_DELAY_FRAMES, MIN_CONTROLLER_INPUT_DELAY_FRAMES, NetplayCoreDescriptor,
    NetplayGameDescriptor, NetplaySessionDescriptor,
};
pub use session_descriptor_error::SessionDescriptorError;
pub use session_mode::NetplaySessionMode;
pub use session_pause::{
    SessionPauseHolder, SessionPauseReason, SessionPauseState, SessionPauseView,
};
pub use snapshot::{SnapshotChunk, SnapshotLimits, SnapshotManifest};
pub use snapshot_file_relay::{
    SnapshotFileRelayGrant, SnapshotFileRelayGrantPair, SnapshotFileRelayGrantRole,
};
pub use state_hash::{
    NearbyStateHashMatchView, PlayerStateHashView, StateHashMismatchView, StateHashReport,
};
pub use voice_descriptor::{NetplayVoiceDescriptor, NetplayVoiceMode};
