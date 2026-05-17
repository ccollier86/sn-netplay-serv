//! Frame-numbered input payloads.
//!
//! The server validates ownership and coarse frame bounds but treats the input
//! payload as opaque normalized controller bytes from Desktop.

use crate::limits::MAX_FUTURE_FRAME_DISTANCE;
use crate::rooms::PlayerIndex;
use serde::{Deserialize, Serialize};

/// One player's normalized input for one emulation frame.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputFrame {
    /// Server-assigned player index.
    pub player_index: PlayerIndex,
    /// Canonical emulation frame number.
    pub frame: u64,
    /// Compact controller payload produced by ShadowBoy Desktop.
    pub payload: Vec<u8>,
}

/// Input validation limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputFrameLimits {
    /// Maximum accepted distance beyond the room's current frame.
    pub max_future_frame_distance: u64,
}

impl Default for InputFrameLimits {
    fn default() -> Self {
        Self {
            max_future_frame_distance: MAX_FUTURE_FRAME_DISTANCE,
        }
    }
}
