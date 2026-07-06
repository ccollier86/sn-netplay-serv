//! WebSocket transport for active netplay rooms.
//!
//! Transport code owns socket reads, socket writes, and conversion between room
//! events and wire messages. It does not own room domain rules.

mod websocket_input_session;
mod websocket_join;
mod websocket_lobby_file_relay_grants;
mod websocket_lobby_outbound;
mod websocket_lobby_rom_relay_handler;
mod websocket_lobby_startup_state_relay_handler;
mod websocket_lobby_session;
mod websocket_message_handler;
mod websocket_outbound;
mod websocket_peer_close;
mod websocket_public_lobbies_session;
mod websocket_rom_relay_handler;
mod websocket_session;
mod websocket_snapshot_file_relay_handler;
mod websocket_voice_handler;

pub use websocket_input_session::handle_websocket_input_session;
pub use websocket_join::{
    WebSocketInputJoinRequest, WebSocketJoinRequest, WebSocketJoinRole, WebSocketLobbyJoinRequest,
};
pub use websocket_lobby_session::handle_websocket_lobby_session;
pub use websocket_public_lobbies_session::handle_public_lobbies_websocket_session;
pub use websocket_session::handle_websocket_session;
