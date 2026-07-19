//! Centralized protocol and room limits.
//!
//! Keeping limits in one module avoids hidden transport or domain assumptions.
//! Values here are intentionally conservative until real-world measurements are
//! available from ShadowBoy Desktop.

use std::time::Duration;

/// Default number of players supported by the MVP room model.
pub const MVP_ROOM_CAPACITY: u8 = 2;

/// Maximum number of future frames accepted from a client.
///
/// Controller netplay clients may run with prediction and rollback, so a client
/// can legitimately advance beyond the relay's canonical room frame while the
/// other side's delayed input is still in flight. This limit is an abuse guard,
/// not a lockstep pacing rule.
pub const MAX_FUTURE_FRAME_DISTANCE: u64 = 240;

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

/// Exact byte width of `shadowboy-retropad-v1-le` controller input.
pub const V5_RETROPAD_INPUT_BYTES: usize = 10;

/// Maximum accepted lead over the protocol v5 host-open cursor.
pub const V5_MAX_FUTURE_FRAME_DISTANCE: u64 = 96;

/// Minimum local-input retention required by protocol v5 clients.
pub const V5_RETAINED_INPUT_FRAMES: usize = 128;

/// Maximum size for one snapshot chunk relayed through the server.
pub const MAX_SNAPSHOT_CHUNK_BYTES: usize = 256 * 1024;

/// Maximum total snapshot bytes accepted for sync snapshots.
pub const MAX_SNAPSHOT_BYTES: u64 = 100 * 1024 * 1024;

/// Maximum virtual link-cable packet payload relayed through the server.
pub const MAX_LINK_CABLE_PACKET_BYTES: usize = 4 * 1024;

/// Time a room can wait for a guest before expiring.
pub const ROOM_JOIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How often the in-memory registry scans for waiting rooms to expire.
pub const ROOM_EXPIRATION_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// How often controller-netplay rooms release one canonical server frame.
///
/// The relay clock is the NOINPUT-style authority for controller netplay. Keep
/// it at 60 Hz instead of 16 ms/62.5 Hz so clients do not slowly drift behind
/// the server under normal NTSC cores.
pub const ROOM_FRAME_CLOCK_INTERVAL: Duration = Duration::from_nanos(16_666_667);

/// Minimum future delay before v2 scheduled gameplay release.
pub const SCHEDULED_START_MINIMUM_DELAY: Duration = Duration::from_secs(3);
