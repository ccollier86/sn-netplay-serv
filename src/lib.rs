//! Library surface for the ShadowBoy netplay relay.
//!
//! The library exposes independently testable modules for configuration, auth,
//! HTTP routing, protocol types, and room coordination. The binary entry point
//! is intentionally thin.

pub mod analytics;
pub mod auth;
pub mod config;
pub mod file_relay;
pub mod http;
pub mod limits;
pub mod lobbies;
pub mod observability;
pub mod protocol;
pub mod rate_limit;
pub mod rooms;
pub mod transport;
pub mod voice;
