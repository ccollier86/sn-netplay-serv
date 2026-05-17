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

/// Maximum size for one snapshot chunk relayed through the server.
pub const MAX_SNAPSHOT_CHUNK_BYTES: usize = 256 * 1024;

/// Maximum total snapshot bytes accepted for MVP sync.
pub const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

/// Time a room can wait for a guest before expiring.
pub const ROOM_JOIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How often the in-memory registry scans for waiting rooms to expire.
pub const ROOM_EXPIRATION_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
