//! Postgres queries for file-relay transfer diagnostics.

use crate::analytics::config::AnalyticsConfig;
use crate::analytics::file_relay_schema;
use crate::observability::{PostgresConnectError, connect_postgres};
use tokio::task::JoinHandle;
use tokio_postgres::{Client, SimpleQueryMessage};

/// Sanitized file-relay transfer event row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRelayEventRow {
    pub timestamp_ms: u64,
    pub transfer_id: String,
    pub room_id: String,
    pub kind: String,
    pub phase: String,
    pub status: String,
    pub size_bytes: u64,
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
    pub chunk_index: Option<u64>,
    pub chunk_count: u64,
    pub detail: String,
}

/// Analytics facade for the shared file-relay table.
pub struct FileRelayAnalyticsDb {
    client: Client,
    _task: JoinHandle<()>,
    table: String,
}

impl FileRelayAnalyticsDb {
    /// Opens the configured Postgres analytics database.
    pub async fn connect(config: AnalyticsConfig) -> Result<Self, FileRelayAnalyticsDbError> {
        let connection = connect_postgres(&config.dsn).await?;

        Ok(Self {
            client: connection.client,
            _task: connection.task,
            table: config.file_relay_transfer_events_table,
        })
    }

    /// Creates file-relay telemetry tables and indexes.
    pub async fn apply_schema(&self) -> Result<(), FileRelayAnalyticsDbError> {
        for query in file_relay_schema::create_table_queries(&self.table) {
            self.client.simple_query(&query).await?;
        }

        Ok(())
    }

    /// Returns recent file-relay events, newest first.
    pub async fn recent_events(
        &self,
        filter: FileRelayEventFilter,
        limit: usize,
    ) -> Result<Vec<FileRelayEventRow>, FileRelayAnalyticsDbError> {
        let where_clause = filter.where_clause();
        let query = format!(
            "SELECT timestamp_ms::text, transfer_id, room_id, kind, phase, status, \
             size_bytes::text, uploaded_bytes::text, downloaded_bytes::text, \
             COALESCE(chunk_index::text, ''), chunk_count::text, detail \
             FROM {} {where_clause} ORDER BY timestamp_ms DESC LIMIT {}",
            file_relay_schema::quote_identifier(&self.table),
            limit.clamp(1, 1000),
        );
        let rows = simple_rows(self.client.simple_query(&query).await?);

        rows.into_iter()
            .map(file_relay_event_row)
            .collect::<Result<Vec<_>, _>>()
    }
}

/// Optional filters for file-relay diagnostics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileRelayEventFilter {
    pub room_id: Option<String>,
    pub transfer_id: Option<String>,
}

impl FileRelayEventFilter {
    fn where_clause(&self) -> String {
        let mut clauses = Vec::new();
        if let Some(room_id) = &self.room_id {
            clauses.push(format!("room_id = '{}'", escape_sql(room_id)));
        }
        if let Some(transfer_id) = &self.transfer_id {
            clauses.push(format!("transfer_id = '{}'", escape_sql(transfer_id)));
        }

        if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        }
    }
}

fn file_relay_event_row(
    row: tokio_postgres::SimpleQueryRow,
) -> Result<FileRelayEventRow, FileRelayAnalyticsDbError> {
    Ok(FileRelayEventRow {
        timestamp_ms: parse_u64(required_cell(&row, 0, "timestamp_ms")?, "timestamp_ms")?,
        transfer_id: required_cell(&row, 1, "transfer_id")?.to_string(),
        room_id: required_cell(&row, 2, "room_id")?.to_string(),
        kind: required_cell(&row, 3, "kind")?.to_string(),
        phase: required_cell(&row, 4, "phase")?.to_string(),
        status: required_cell(&row, 5, "status")?.to_string(),
        size_bytes: parse_u64(required_cell(&row, 6, "size_bytes")?, "size_bytes")?,
        uploaded_bytes: parse_u64(required_cell(&row, 7, "uploaded_bytes")?, "uploaded_bytes")?,
        downloaded_bytes: parse_u64(
            required_cell(&row, 8, "downloaded_bytes")?,
            "downloaded_bytes",
        )?,
        chunk_index: parse_optional_u64(required_cell(&row, 9, "chunk_index")?, "chunk_index")?,
        chunk_count: parse_u64(required_cell(&row, 10, "chunk_count")?, "chunk_count")?,
        detail: required_cell(&row, 11, "detail")?.to_string(),
    })
}

fn simple_rows(messages: Vec<SimpleQueryMessage>) -> Vec<tokio_postgres::SimpleQueryRow> {
    messages
        .into_iter()
        .filter_map(|message| match message {
            SimpleQueryMessage::Row(row) => Some(row),
            _ => None,
        })
        .collect()
}

fn escape_sql(value: &str) -> String {
    value.replace('\'', "''")
}

fn required_cell<'a>(
    row: &'a tokio_postgres::SimpleQueryRow,
    index: usize,
    field: &'static str,
) -> Result<&'a str, FileRelayAnalyticsDbError> {
    row.get(index)
        .ok_or(FileRelayAnalyticsDbError::MalformedRow { field })
}

fn parse_u64(value: &str, field: &'static str) -> Result<u64, FileRelayAnalyticsDbError> {
    value
        .parse()
        .map_err(|_| FileRelayAnalyticsDbError::MalformedNumber {
            field,
            value: value.to_string(),
        })
}

fn parse_optional_u64(
    value: &str,
    field: &'static str,
) -> Result<Option<u64>, FileRelayAnalyticsDbError> {
    if value.is_empty() {
        return Ok(None);
    }

    parse_u64(value, field).map(Some)
}

/// File-relay analytics database failure.
#[derive(Debug, thiserror::Error)]
pub enum FileRelayAnalyticsDbError {
    #[error(transparent)]
    Connect(#[from] PostgresConnectError),
    #[error("file relay analytics query failed")]
    Postgres(#[from] tokio_postgres::Error),
    #[error("file relay analytics row missed field {field}")]
    MalformedRow { field: &'static str },
    #[error("file relay analytics invalid number for {field}: {value}")]
    MalformedNumber { field: &'static str, value: String },
}
