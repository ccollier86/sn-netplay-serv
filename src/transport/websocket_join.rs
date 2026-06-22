//! Join request values for room WebSocket sessions.
//!
//! These values are built by HTTP route handlers after auth succeeds and then
//! moved into the upgraded socket task.

use crate::auth::VerifiedLicense;
use crate::lobbies::LobbyClientCapabilities;
use crate::rooms::{InviteCode, PlayerIndex};
use serde::Deserialize;

/// Requested role for a room WebSocket join.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum WebSocketJoinRole {
    /// Attach as the room host and Player 1.
    Host,
    /// Join as the room guest and Player 2.
    #[default]
    Guest,
}

/// Authenticated room WebSocket join request.
#[derive(Clone, Debug)]
pub struct WebSocketJoinRequest {
    /// Room invite code.
    pub invite_code: InviteCode,
    /// Requested socket role.
    pub role: WebSocketJoinRole,
    /// Player slot being reclaimed during reconnect.
    pub reconnect_player_index: Option<PlayerIndex>,
    /// Room epoch supplied for reconnect.
    pub reconnect_room_epoch: Option<u64>,
    /// Opaque resume token supplied for reconnect.
    pub resume_token: Option<String>,
    /// Whether this socket can use file relay for large sync state.
    pub supports_state_file_relay: bool,
    /// Whether this socket can use file relay for temporary direct-invite ROMs.
    pub supports_rom_file_relay: bool,
    /// Whether this socket can use v2 scheduled synchronized start.
    pub supports_scheduled_start: bool,
    /// Whether this socket can answer v2 clock-sample requests.
    pub supports_clock_sync: bool,
    /// Whether this socket can use the v2 fast binary input relay.
    pub supports_fast_input_relay: bool,
    /// Verified Desktop install/license identity.
    pub license: VerifiedLicense,
}

/// Authenticated binary input WebSocket join request.
#[derive(Clone, Debug)]
pub struct WebSocketInputJoinRequest {
    /// Room invite code.
    pub invite_code: InviteCode,
    /// Player slot attaching this input socket.
    pub player_index: PlayerIndex,
    /// Room epoch observed by the client.
    pub room_epoch: u64,
    /// Session epoch observed by the client.
    pub session_epoch: u64,
    /// Opaque input socket token returned by the control socket.
    pub input_socket_token: String,
    /// Verified Desktop install/license identity.
    pub license: VerifiedLicense,
}

/// Authenticated lobby WebSocket join request.
#[derive(Clone, Debug)]
pub struct WebSocketLobbyJoinRequest {
    /// Lobby invite code.
    pub invite_code: InviteCode,
    /// Optional display name for lobby UI.
    pub display_name: Option<String>,
    /// Client feature support.
    pub capabilities: LobbyClientCapabilities,
    /// Player slot being reclaimed during reconnect.
    pub reconnect_player_index: Option<PlayerIndex>,
    /// Lobby epoch supplied for reconnect.
    pub reconnect_lobby_epoch: Option<u64>,
    /// Opaque resume token supplied for reconnect.
    pub resume_token: Option<String>,
    /// Verified Desktop install/license identity.
    pub license: VerifiedLicense,
}
