//! Strong identifiers used by room domain code.
//!
//! These wrappers avoid passing raw integers and UUIDs through slot-sensitive
//! logic where mixups would create protocol bugs.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Internal room identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomId(Uuid);

impl RoomId {
    /// Generates a new unique room identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the raw UUID used by storage and wire diagnostics.
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for RoomId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Per-socket connection identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConnectionId(Uuid);

impl ConnectionId {
    /// Generates a new unique connection identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero-based netplay player index.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerIndex(u8);

impl PlayerIndex {
    /// Host player for the two-player MVP.
    pub const ONE: Self = Self(0);
    /// Guest player for the two-player MVP.
    pub const TWO: Self = Self(1);

    /// Creates a player index if it fits the room capacity.
    pub fn new(value: u8, max_players: u8) -> Option<Self> {
        (value < max_players).then_some(Self(value))
    }

    /// Returns the zero-based value used by protocol messages.
    pub fn zero_based(self) -> u8 {
        self.0
    }

    /// Returns the one-based value shown to users.
    pub fn display_number(self) -> u8 {
        self.0 + 1
    }
}
