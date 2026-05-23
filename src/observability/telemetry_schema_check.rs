//! Startup schema checks for durable telemetry.
//!
//! The server fails fast when a configured analytics sink cannot be reached or
//! initialized. That keeps gameplay writes cheap and avoids repeated schema work
//! in the telemetry drain.

use crate::config::{TelemetryConfig, TelemetrySinkConfig};
use crate::observability::connect_postgres;
use crate::observability::postgres_schema;
use tracing::info;

/// Applies and verifies the configured durable telemetry schema at startup.
pub async fn ensure_telemetry_schema(config: &TelemetryConfig) -> Result<(), TelemetrySchemaError> {
    match &config.sink {
        TelemetrySinkConfig::Disabled => {
            info!("durable telemetry disabled");
            Ok(())
        }
        TelemetrySinkConfig::Postgres(postgres) => {
            let connection = connect_postgres(&postgres.dsn).await?;

            for query in postgres_schema::create_table_queries(&postgres.tables) {
                connection.client.simple_query(&query).await?;
            }

            connection
                .client
                .query_one(
                    "SELECT to_regclass($1), to_regclass($2)",
                    &[
                        &postgres.tables.events,
                        &postgres.tables.performance_samples,
                    ],
                )
                .await?;

            info!(
                events_table = %postgres.tables.events,
                performance_table = %postgres.tables.performance_samples,
                "postgres telemetry schema ready"
            );
            Ok(())
        }
    }
}

/// Startup telemetry schema failure.
#[derive(Debug, thiserror::Error)]
pub enum TelemetrySchemaError {
    /// Postgres connection setup failed.
    #[error(transparent)]
    Connect(#[from] crate::observability::PostgresConnectError),
    /// Schema query failed.
    #[error("telemetry schema query failed: {0}")]
    Query(#[from] tokio_postgres::Error),
}
