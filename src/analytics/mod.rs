//! Operator analytics helpers for netplay telemetry.

mod cli;
mod config;
mod file_relay_query;
mod file_relay_report;
mod file_relay_schema;
mod live;
mod query;
mod report;

pub use cli::run_cli;
