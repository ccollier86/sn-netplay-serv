//! Join request values for room WebSocket sessions.
//!
//! These values are built by HTTP route handlers after auth succeeds and then
//! moved into the upgraded socket task.

use crate::auth::VerifiedLicense;
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
    /// Verified Desktop install/license identity.
    pub license: VerifiedLicense,
}
