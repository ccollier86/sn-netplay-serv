//! Library surface for the ShadowBoy netplay relay.
//!
//! The library exposes independently testable modules for configuration, auth,
//! HTTP routing, protocol types, and room coordination. The binary entry point
//! is intentionally thin.

pub mod auth;
pub mod config;
pub mod http;
pub mod limits;
pub mod protocol;
pub mod rooms;
pub mod transport;
