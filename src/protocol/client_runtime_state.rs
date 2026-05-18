//! Client runtime state reported through heartbeats.
//!
//! These values describe the local emulator/netplay loop from the client's
//! point of view. They are protocol DTOs only; room domain code maps them onto
//! player-slot state.

use serde::{Deserialize, Serialize};

/// Local runtime state reported by a connected client.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientRuntimeState {
    /// Socket is connected but the runtime has not entered gameplay.
    Connected,
    /// Client is checking compatibility.
    CheckingCompatibility,
    /// Client is syncing or loading state.
    Syncing,
    /// Client has loaded required sync state.
    Ready,
    /// Client is running active netplay.
    Playing,
    /// Client is moving toward a scheduled pause frame.
    Pausing,
    /// Client is paused at a coordinated frame.
    Paused,
    /// Client is reconnecting after transport loss.
    Reconnecting,
    /// Client has disconnected.
    Disconnected,
}
