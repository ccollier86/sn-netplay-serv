//! Production observability helpers.
//!
//! This module owns process metrics and tracing setup. It does not know room
//! mutation rules or HTTP authentication details.

mod metrics;
mod postgres_connection;
mod postgres_dsn;
pub(crate) mod postgres_schema;
mod postgres_telemetry_writer;
mod telemetry;
mod telemetry_event;
mod telemetry_schema_check;
mod tracing_setup;

pub use metrics::{InMemoryMetrics, MetricsRecorder, MetricsSnapshot};
pub(crate) use postgres_connection::{PostgresConnectError, connect_postgres};
pub use postgres_dsn::{PostgresDsn, PostgresTlsMode};
pub use postgres_schema::PostgresTableNames;
pub(crate) use postgres_telemetry_writer::PostgresTelemetryWriter;
pub use telemetry::spawn_telemetry_sink;
pub(crate) use telemetry_event::{
    NetplayLobbyTelemetryEvent, NetplayPerformanceSample, NetplayTelemetryEvent,
    NetplayTelemetryRecord,
};
pub use telemetry_schema_check::ensure_telemetry_schema;
pub use tracing_setup::init_tracing;
