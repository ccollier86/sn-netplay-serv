//! Room coordination for invite-code netplay sessions.
//!
//! The room modules own room lifecycle, slot assignment, player status, and
//! frame-input validation. They do not parse HTTP requests or call the license
//! authority.

mod errors;
mod ids;
mod invite_code;
mod player_slot;
mod room;
mod room_event;
mod room_expiration_task;
mod room_registry;

pub use errors::RoomError;
pub use ids::{ConnectionId, PlayerIndex, RoomId};
pub use invite_code::{InviteCode, InviteCodeGenerator, UuidInviteCodeGenerator};
pub use player_slot::{PlayerRole, PlayerSlot, PlayerStatus};
pub use room::{NetplayRoom, RoomStatus, RoomView};
pub use room_event::RoomEvent;
pub use room_expiration_task::spawn_room_expiration_task;
pub use room_registry::{InMemoryRoomRegistry, RoomEventReceiver, RoomJoin, RoomRegistry};
