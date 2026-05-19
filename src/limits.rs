//! Centralized protocol and room limits.
//!
//! Keeping limits in one module avoids hidden transport or domain assumptions.
//! Values here are intentionally conservative until real-world measurements are
//! available from ShadowBoy Desktop.

use std::time::Duration;

/// Default number of players supported by the MVP room model.
pub const MVP_ROOM_CAPACITY: u8 = 2;

/// Maximum number of future frames accepted from a client.
pub const MAX_FUTURE_FRAME_DISTANCE: u64 = 6;

/// Maximum create-room JSON request size.
pub const MAX_CREATE_ROOM_BODY_BYTES: usize = 16 * 1024;

/// Maximum accepted WebSocket text message size.
pub const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

/// Maximum accepted WebSocket frame size.
pub const MAX_WEBSOCKET_FRAME_BYTES: usize = 2 * 1024 * 1024;

/// Maximum controller input frames accepted in one binary batch.
pub const MAX_INPUT_BATCH_FRAMES: usize = 4;

/// Maximum accepted binary input-batch message size.
pub const MAX_INPUT_BATCH_BYTES: usize = 8 * 1024;

/// Maximum size for one snapshot chunk relayed through the server.
pub const MAX_SNAPSHOT_CHUNK_BYTES: usize = 256 * 1024;

/// Maximum total snapshot bytes accepted for MVP sync.
pub const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

/// Maximum virtual link-cable packet payload relayed through the server.
pub const MAX_LINK_CABLE_PACKET_BYTES: usize = 4 * 1024;

/// Time a room can wait for a guest before expiring.
pub const ROOM_JOIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How often the in-memory registry scans for waiting rooms to expire.
pub const ROOM_EXPIRATION_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
