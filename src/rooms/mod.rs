//! Room coordination for invite-code netplay sessions.
//!
//! The room modules own room lifecycle, slot assignment, player status, and
//! frame-input validation. They do not parse HTTP requests or call the license
//! authority.

mod errors;
mod ids;
mod input_frame_acceptance;
mod invite_code;
mod link_cable_room_state;
mod player_slot;
mod room;
mod room_controller_netplay_ops;
mod room_event;
mod room_expiration_task;
mod room_link_cable_ops;
mod room_registry;
mod room_registry_snapshot;
mod room_status;
mod room_view;
mod session_pause_state;
mod snapshot_transfer;
mod stored_room;

pub use errors::RoomError;
pub use ids::{ConnectionId, PlayerIndex, RoomId};
pub use input_frame_acceptance::InputFrameAcceptance;
pub use invite_code::{InviteCode, InviteCodeGenerator, UuidInviteCodeGenerator};
pub(crate) use link_cable_room_state::LinkCableRoomState;
pub use player_slot::{PlayerRole, PlayerSlot, PlayerStatus};
pub use room::NetplayRoom;
pub use room_controller_netplay_ops::SessionResumeOutcome;
pub use room_event::RoomEvent;
pub use room_expiration_task::spawn_room_expiration_task;
pub use room_registry::{InMemoryRoomRegistry, RoomEventReceiver, RoomJoin, RoomRegistry};
pub use room_registry_snapshot::RoomRegistrySnapshot;
pub use room_status::RoomStatus;
pub use room_view::{PlayerSlotView, RoomView};
pub(crate) use session_pause_state::SessionPauseStateTracker;
pub(crate) use snapshot_transfer::SnapshotTransferState;
