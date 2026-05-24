//! Room coordination for invite-code netplay sessions.
//!
//! The room modules own room lifecycle, slot assignment, player status, and
//! frame-input validation. They do not parse HTTP requests or call the license
//! authority.

mod adaptive_input_delay_policy;
mod clock;
mod errors;
mod ids;
mod input_frame_acceptance;
mod input_frame_relay_buffer;
mod invite_code;
mod link_cable_room_state;
mod player_slot;
mod recovery_config;
mod resume_token;
mod room;
mod room_adaptive_input_delay_ops;
mod room_connection_diagnostics;
mod room_connection_ops;
mod room_controller_netplay_ops;
mod room_debug_event;
mod room_event;
mod room_expiration_task;
mod room_frame_clock_ops;
mod room_frame_clock_task;
mod room_input_connection_ops;
mod room_join;
mod room_link_cable_ops;
mod room_performance_sample;
mod room_registry;
mod room_registry_snapshot;
mod room_registry_trait;
mod room_state_hash_ops;
mod room_status;
mod room_view;
mod room_voice;
mod room_voice_ops;
mod session_pause_state;
mod snapshot_transfer;
mod stored_room;

pub(crate) use adaptive_input_delay_policy::AdaptiveInputDelayPolicy;
pub use clock::{Clock, SystemClock};
pub use errors::RoomError;
pub use ids::{ConnectionId, PlayerIndex, RoomId};
pub use input_frame_acceptance::InputFrameAcceptance;
pub(crate) use input_frame_relay_buffer::InputFrameRelayBuffer;
pub use invite_code::{InviteCode, InviteCodeGenerator, UuidInviteCodeGenerator};
pub(crate) use link_cable_room_state::LinkCableRoomState;
pub use player_slot::{PlayerRole, PlayerRuntimeState, PlayerSlot, PlayerStatus};
pub use recovery_config::RoomRecoveryConfig;
pub use resume_token::{
    ResumeToken, ResumeTokenGenerator, ResumeTokenHash, UuidResumeTokenGenerator, hash_resume_token,
};
pub use room::NetplayRoom;
pub use room_controller_netplay_ops::{SessionPauseReachedOutcome, SessionResumeOutcome};
pub use room_debug_event::{
    NoopRoomDebugEventSink, RoomDebugEvent, RoomDebugEventLog, RoomDebugEventSink,
    current_timestamp_ms,
};
pub use room_event::{RoomEvent, RoomInputEvent};
pub use room_expiration_task::spawn_room_expiration_task;
pub use room_frame_clock_task::spawn_room_frame_clock_task;
pub use room_join::RoomJoin;
pub use room_performance_sample::RoomPerformanceSample;
pub use room_registry::InMemoryRoomRegistry;
pub use room_registry_snapshot::RoomRegistrySnapshot;
pub use room_registry_trait::{RoomEventReceiver, RoomInputEventReceiver, RoomRegistry};
pub(crate) use room_state_hash_ops::StateHashEvaluation;
pub use room_status::RoomStatus;
pub use room_view::{PlayerFrameCursorView, PlayerSlotView, RoomFrameClockView, RoomView};
pub(crate) use room_voice::RoomVoiceState;
pub use room_voice::{PlayerVoiceJoinGrant, RoomVoiceStatus, RoomVoiceView};
pub(crate) use session_pause_state::SessionPauseStateTracker;
pub(crate) use snapshot_transfer::SnapshotTransferState;
