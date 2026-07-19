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
mod compatibility_v5;
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
mod protocol_rollout;
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
mod state_recovery;
mod strict_input;
mod voice_descriptor;

#[cfg(test)]
mod netplay_v5_spec_tests;

pub use client_kind::NetplayClientKind;
pub use client_message::ClientMessage;
pub use client_network_quality::ClientNetworkQualityReport;
pub use client_runtime_state::ClientRuntimeState;
pub use clock_sync::{
    ClockSyncEstimate, ClockSyncPing, ClockSyncPong, ClockSyncSample, ClockSyncSampleRequest,
};
pub use compatibility::{CompatibilityFingerprint, CompatibilityMismatch};
pub use compatibility_v5::{DeterminismProfileV5, StateDigestMode};
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
pub use lobby_file_relay::{
    LobbyFileRelayGrant, LobbyFileRelayGrantPair, LobbyFileRelayGrantRole,
    LobbyFileRelayMaterialKind, LobbyStartupStateRestorePolicy, LobbyStartupStateTransferMetadata,
};
pub use lobby_messages::{LobbyClientMessage, LobbyServerMessage};
pub use netplay_protocol::{
    LEGACY_NETPLAY_PROTOCOL_VERSION, MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION,
    NETPLAY_PROTOCOL_VERSION, NetplayProtocolView, ProtocolVersionError,
    negotiate_client_protocol_version, validate_client_protocol_version,
    validate_room_protocol_version,
};
pub use protocol_rollout::NetplayProtocolRolloutPolicy;
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
pub use state_recovery::{StateRecoveryPhase, StateRecoveryPin, StateRecoveryView};
pub use strict_input::{
    AcceptedInputCursor, HostFrameOpen, InputCursorAck, InputCursorNack, InputCursorNackReason,
    InputCursorResponse, RetropadInputPayload, ServerFrameReleaseV5, StrictInputBatch,
    StrictInputCodecError, V5_INPUT_CODEC_ID, V5_INPUT_PREDICTOR_ID, decode_host_frame_open,
    decode_input_cursor_ack, decode_input_cursor_nack, decode_server_frame_release_v5,
    decode_strict_input_batch, encode_host_frame_open, encode_input_cursor_ack,
    encode_input_cursor_nack, encode_server_frame_release_v5, encode_strict_input_batch,
};
pub use voice_descriptor::{NetplayVoiceDescriptor, NetplayVoiceMode};
