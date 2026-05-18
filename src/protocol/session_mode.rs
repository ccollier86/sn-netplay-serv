//! Session mode identifiers for relay rooms.
//!
//! Modes separate lockstep controller netplay from virtual link-cable sessions.
//! The default preserves compatibility with existing Desktop clients that do
//! not yet send an explicit mode field.

use serde::{Deserialize, Serialize};

/// High-level room behavior selected by the host.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetplaySessionMode {
    /// Current save-state sync plus lockstep input timeline behavior.
    #[default]
    ControllerNetplay,
    /// Independent emulator instances connected by virtual link-cable traffic.
    LinkCable,
}
