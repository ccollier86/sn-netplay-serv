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

/// Room WebSocket join request classified at the HTTP authorization boundary.
#[derive(Clone)]
pub struct WebSocketJoinRequest {
    /// Room invite code.
    pub invite_code: InviteCode,
    /// Protected initial join or scoped capability resume.
    pub intent: WebSocketRoomJoinIntent,
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
}

/// Authorization intent for a room control socket.
#[derive(Clone)]
pub enum WebSocketRoomJoinIntent {
    /// Initial host/guest attachment backed by protected client authorization.
    Initial {
        /// Requested room role.
        role: WebSocketJoinRole,
        /// Verified installation/license identity.
        license: VerifiedLicense,
        /// Whether this provisional socket will transfer to a runner.
        runner_handoff: bool,
    },
    /// Scoped resume capability for an existing player slot.
    Resume {
        /// Player slot being reclaimed.
        player_index: PlayerIndex,
        /// Room epoch captured with the capability.
        room_epoch: u64,
        /// Opaque one-time resume capability.
        resume_token: String,
    },
}

impl std::fmt::Debug for WebSocketRoomJoinIntent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initial {
                role,
                runner_handoff,
                ..
            } => formatter
                .debug_struct("Initial")
                .field("role", role)
                .field("runner_handoff", runner_handoff)
                .finish(),
            Self::Resume {
                player_index,
                room_epoch,
                ..
            } => formatter
                .debug_struct("Resume")
                .field("player_index", player_index)
                .field("room_epoch", room_epoch)
                .field("resume_token", &"<redacted>")
                .finish(),
        }
    }
}

impl std::fmt::Debug for WebSocketJoinRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebSocketJoinRequest")
            .field("invite_code", &self.invite_code)
            .field("intent", &self.intent)
            .finish_non_exhaustive()
    }
}

/// Capability-authenticated binary input WebSocket join request.
#[derive(Clone)]
pub struct WebSocketInputJoinRequest {
    /// Room invite code.
    pub invite_code: InviteCode,
    /// Exact room protocol validated before the WebSocket upgrade.
    pub protocol_version: u16,
    /// Player slot attaching this input socket.
    pub player_index: PlayerIndex,
    /// Room epoch observed by the client.
    pub room_epoch: u64,
    /// Session epoch observed by the client.
    pub session_epoch: u64,
    /// Opaque input socket token returned by the control socket.
    pub input_socket_token: String,
}

impl std::fmt::Debug for WebSocketInputJoinRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebSocketInputJoinRequest")
            .field("invite_code", &self.invite_code)
            .field("protocol_version", &self.protocol_version)
            .field("player_index", &self.player_index)
            .field("room_epoch", &self.room_epoch)
            .field("session_epoch", &self.session_epoch)
            .field("input_socket_token", &"<redacted>")
            .finish()
    }
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

#[cfg(test)]
mod tests {
    use super::{WebSocketInputJoinRequest, WebSocketRoomJoinIntent};
    use crate::rooms::{InviteCode, PlayerIndex};

    #[test]
    fn capability_request_debug_output_redacts_tokens() {
        let resume = WebSocketRoomJoinIntent::Resume {
            player_index: PlayerIndex::ONE,
            room_epoch: 7,
            resume_token: "secret-resume-capability".to_string(),
        };
        let input = WebSocketInputJoinRequest {
            invite_code: InviteCode::parse("AB23-CD").expect("invite"),
            protocol_version: crate::protocol::NETPLAY_PROTOCOL_VERSION,
            player_index: PlayerIndex::ONE,
            room_epoch: 7,
            session_epoch: 9,
            input_socket_token: "secret-input-capability".to_string(),
        };

        assert!(!format!("{resume:?}").contains("secret-resume-capability"));
        assert!(!format!("{input:?}").contains("secret-input-capability"));
    }
}
