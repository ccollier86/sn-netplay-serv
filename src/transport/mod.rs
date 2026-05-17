//! WebSocket transport for active netplay rooms.
//!
//! Transport code owns socket reads, socket writes, and conversion between room
//! events and wire messages. It does not own room domain rules.

mod websocket_join;
mod websocket_message_handler;
mod websocket_outbound;
mod websocket_session;

pub use websocket_join::{WebSocketJoinRequest, WebSocketJoinRole};
pub use websocket_session::handle_websocket_session;
