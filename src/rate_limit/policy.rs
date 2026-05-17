//! Typed rate-limit policy values.
//!
//! Policy stays separate from the limiter implementation so configuration can
//! parse simple numbers without knowing how request windows are stored.

/// Configurable per-minute request ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitPolicy {
    /// Maximum room creation attempts per key per minute.
    pub create_room_per_minute: u32,
    /// Maximum WebSocket join attempts per key per minute.
    pub websocket_join_per_minute: u32,
    /// Maximum public room status checks per key per minute.
    pub room_status_per_minute: u32,
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            create_room_per_minute: 12,
            websocket_join_per_minute: 30,
            room_status_per_minute: 120,
        }
    }
}

impl RateLimitPolicy {
    /// Returns the ceiling for one protected action.
    pub fn limit_for(self, action: RateLimitAction) -> u32 {
        match action {
            RateLimitAction::CreateRoom => self.create_room_per_minute,
            RateLimitAction::WebSocketJoin => self.websocket_join_per_minute,
            RateLimitAction::RoomStatus => self.room_status_per_minute,
        }
    }
}

/// Action buckets independently rate limited by the relay.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RateLimitAction {
    /// `POST /v1/rooms`.
    CreateRoom,
    /// `GET /v1/ws`.
    WebSocketJoin,
    /// `GET /v1/rooms/{invite_code}/status`.
    RoomStatus,
}

impl RateLimitAction {
    /// Stable metric/log label for this action.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateRoom => "create_room",
            Self::WebSocketJoin => "websocket_join",
            Self::RoomStatus => "room_status",
        }
    }
}
