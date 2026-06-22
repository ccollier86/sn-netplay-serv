//! Client-advertised room transport capabilities.
//!
//! This value object keeps connection APIs from passing loose boolean groups
//! while preserving v1 defaults for clients that do not advertise v2 features.

/// Optional features supported by one connected Desktop client.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClientTransportCapabilities {
    /// Client can use large save-state file relay.
    pub supports_state_file_relay: bool,
    /// Client can use temporary ROM file relay.
    pub supports_rom_file_relay: bool,
    /// Client can wait for server-scheduled deterministic start.
    pub supports_scheduled_start: bool,
    /// Client can answer startup clock-sample requests.
    pub supports_clock_sync: bool,
    /// Client can send and receive `SBI2` fast input records.
    pub supports_fast_input_relay: bool,
}
