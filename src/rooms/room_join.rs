//! Join result values returned by room storage.
//!
//! These values are transport-neutral. WebSocket sessions decide how to encode
//! them into protocol messages.

use crate::rooms::{PlayerIndex, PlayerVoiceJoinGrant, RoomView};

/// Result returned when a socket joins or rejoins a room.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomJoin {
    /// Player index assigned to the socket connection.
    pub player_index: PlayerIndex,
    /// Opaque reconnect token sent only to this player.
    pub resume_token: String,
    /// Opaque token used to attach the binary input socket.
    ///
    /// Link-cable rooms use their dedicated private data plane and therefore
    /// never receive this controller-input capability.
    pub input_socket_token: Option<String>,
    /// Optional player-specific voice grant. Never broadcast this in room views.
    pub voice: Option<PlayerVoiceJoinGrant>,
    /// Room state immediately after the join.
    pub room: RoomView,
}
