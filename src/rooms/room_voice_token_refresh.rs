//! Result returned after a player refreshes their voice token.
//!
//! The refreshed grant is private to the requesting player. The room view is
//! included only so transports can echo current epochs on the response.

use crate::rooms::{PlayerVoiceJoinGrant, RoomView};

/// Player-specific refreshed voice token and current room metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomVoiceTokenRefresh {
    /// Refreshed private voice grant for the requesting player.
    pub voice: PlayerVoiceJoinGrant,
    /// Current room state at refresh time.
    pub room: RoomView,
}
