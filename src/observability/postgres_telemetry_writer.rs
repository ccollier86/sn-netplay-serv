//! Postgres telemetry writer.
//!
//! The writer runs only on the background telemetry drain task. It keeps
//! database work outside the active room/input relay path.

use crate::config::PostgresTelemetryConfig;
use crate::observability::PostgresConnectError;
use crate::observability::connect_postgres;
use crate::observability::postgres_schema::split_batch;
use crate::observability::telemetry_event::{
    NetplayLobbyTelemetryEvent, NetplayPerformanceSample, NetplayTelemetryEvent,
    NetplayTelemetryRecord,
};
use tokio::task::JoinHandle;
use tokio_postgres::Client;

const INSERT_EVENT_SQL: &str = "\
    INSERT INTO {table} \
    (timestamp_ms, room_id, invite_code, event_seq, room_epoch, session_epoch, kind, detail) \
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)";

const INSERT_SAMPLE_SQL: &str = "\
    INSERT INTO {table} \
    (timestamp_ms, room_id, invite_code, event_seq, room_epoch, session_epoch, player_index, \
     runtime_state, local_frame, canonical_frame, released_frame, next_release_frame, \
     accepted_input_frame, frame_delta, round_trip_ms, jitter_ms, prediction_frames, \
     stall_count, catch_up_frames, late_input_frames, audio_underruns) \
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, \
            $17, $18, $19, $20, $21)";

const INSERT_LOBBY_EVENT_SQL: &str = "\
    INSERT INTO {table} \
    (timestamp_ms, lobby_id, invite_code, event_seq, lobby_epoch, kind, detail) \
    VALUES ($1, $2, $3, $4, $5, $6, $7)";

/// Batched Postgres writer with reconnect-on-failure behavior.
pub struct PostgresTelemetryWriter {
    config: PostgresTelemetryConfig,
    connection: Option<PostgresWriterConnection>,
}

impl PostgresTelemetryWriter {
    /// Creates a writer for the provided DSN.
    pub fn new(config: PostgresTelemetryConfig) -> Self {
        Self {
            config,
            connection: None,
        }
    }

    /// Writes one batch, reconnecting once if the previous client is dead.
    pub async fn write_batch(
        &mut self,
        batch: &[NetplayTelemetryRecord],
    ) -> Result<(), PostgresTelemetryError> {
        if self.connection.is_none() {
            self.connection = Some(self.connect().await?);
        }

        if let Err(error) = self.write_connected(batch).await {
            self.connection.take();
            return Err(error);
        }

        Ok(())
    }

    async fn write_connected(
        &mut self,
        batch: &[NetplayTelemetryRecord],
    ) -> Result<(), PostgresTelemetryError> {
        let grouped = split_batch(batch);
        let connection = self.connection.as_mut().expect("connection");

        if !grouped.events.is_empty() {
            write_events(
                &connection.client,
                &self.config.tables.events,
                &grouped.events,
            )
            .await?;
        }

        if !grouped.lobby_events.is_empty() {
            write_lobby_events(
                &connection.client,
                &self.config.tables.lobby_events,
                &grouped.lobby_events,
            )
            .await?;
        }

        if !grouped.performance_samples.is_empty() {
            write_performance_samples(
                &connection.client,
                &self.config.tables.performance_samples,
                &grouped.performance_samples,
            )
            .await?;
        }

        Ok(())
    }

    async fn connect(&self) -> Result<PostgresWriterConnection, PostgresTelemetryError> {
        let connection = connect_postgres(&self.config.dsn).await?;
        let connection = PostgresWriterConnection {
            client: connection.client,
            _task: connection.task,
        };

        Ok(connection)
    }
}

async fn write_lobby_events(
    client: &Client,
    table: &str,
    events: &[NetplayLobbyTelemetryEvent],
) -> Result<(), PostgresTelemetryError> {
    let sql = INSERT_LOBBY_EVENT_SQL.replace("{table}", &quote_table(table));
    let statement = client.prepare(&sql).await?;

    for event in events {
        let timestamp_ms = u64_to_i64(event.timestamp_ms, "timestamp_ms")?;
        let event_seq = u64_to_i64(event.event_seq, "event_seq")?;
        let lobby_epoch = u64_to_i64(event.lobby_epoch, "lobby_epoch")?;
        let lobby_id = event.lobby_id.as_uuid();

        client
            .execute(
                &statement,
                &[
                    &timestamp_ms,
                    &lobby_id,
                    &event.invite_code,
                    &event_seq,
                    &lobby_epoch,
                    &event.kind,
                    &event.detail,
                ],
            )
            .await?;
    }

    Ok(())
}

async fn write_events(
    client: &Client,
    table: &str,
    events: &[NetplayTelemetryEvent],
) -> Result<(), PostgresTelemetryError> {
    let sql = INSERT_EVENT_SQL.replace("{table}", &quote_table(table));
    let statement = client.prepare(&sql).await?;

    for event in events {
        let timestamp_ms = u64_to_i64(event.timestamp_ms, "timestamp_ms")?;
        let event_seq = u64_to_i64(event.event_seq, "event_seq")?;
        let room_epoch = u64_to_i64(event.room_epoch, "room_epoch")?;
        let session_epoch = u64_to_i64(event.session_epoch, "session_epoch")?;
        let room_id = event.room_id.as_uuid();

        client
            .execute(
                &statement,
                &[
                    &timestamp_ms,
                    &room_id,
                    &event.invite_code,
                    &event_seq,
                    &room_epoch,
                    &session_epoch,
                    &event.kind,
                    &event.detail,
                ],
            )
            .await?;
    }

    Ok(())
}

async fn write_performance_samples(
    client: &Client,
    table: &str,
    samples: &[NetplayPerformanceSample],
) -> Result<(), PostgresTelemetryError> {
    let sql = INSERT_SAMPLE_SQL.replace("{table}", &quote_table(table));
    let statement = client.prepare(&sql).await?;

    for sample in samples {
        let timestamp_ms = u64_to_i64(sample.timestamp_ms, "timestamp_ms")?;
        let event_seq = u64_to_i64(sample.event_seq, "event_seq")?;
        let room_epoch = u64_to_i64(sample.room_epoch, "room_epoch")?;
        let session_epoch = u64_to_i64(sample.session_epoch, "session_epoch")?;
        let local_frame = optional_u64_to_i64(sample.local_frame, "local_frame")?;
        let canonical_frame = u64_to_i64(sample.canonical_frame, "canonical_frame")?;
        let released_frame = optional_u64_to_i64(sample.released_frame, "released_frame")?;
        let next_release_frame = u64_to_i64(sample.next_release_frame, "next_release_frame")?;
        let accepted_input_frame =
            optional_u64_to_i64(sample.accepted_input_frame, "accepted_input_frame")?;
        let round_trip_ms = sample.round_trip_ms.map(i32::try_from).transpose()?;
        let jitter_ms = sample.jitter_ms.map(i32::try_from).transpose()?;
        let prediction_frames = sample.prediction_frames.map(i32::try_from).transpose()?;
        let stall_count = sample.stall_count.map(i32::try_from).transpose()?;
        let catch_up_frames = sample.catch_up_frames.map(i32::try_from).transpose()?;
        let late_input_frames = sample.late_input_frames.map(i32::try_from).transpose()?;
        let audio_underruns = sample.audio_underruns.map(i32::try_from).transpose()?;
        let room_id = sample.room_id.as_uuid();
        let player_index = i16::from(sample.player_index);

        client
            .execute(
                &statement,
                &[
                    &timestamp_ms,
                    &room_id,
                    &sample.invite_code,
                    &event_seq,
                    &room_epoch,
                    &session_epoch,
                    &player_index,
                    &sample.runtime_state,
                    &local_frame,
                    &canonical_frame,
                    &released_frame,
                    &next_release_frame,
                    &accepted_input_frame,
                    &sample.frame_delta,
                    &round_trip_ms,
                    &jitter_ms,
                    &prediction_frames,
                    &stall_count,
                    &catch_up_frames,
                    &late_input_frames,
                    &audio_underruns,
                ],
            )
            .await?;
    }

    Ok(())
}

fn quote_table(table: &str) -> String {
    crate::observability::postgres_schema::quote_identifier(table)
}

fn optional_u64_to_i64(
    value: Option<u64>,
    field: &'static str,
) -> Result<Option<i64>, PostgresTelemetryError> {
    value.map(|value| u64_to_i64(value, field)).transpose()
}

fn u64_to_i64(value: u64, field: &'static str) -> Result<i64, PostgresTelemetryError> {
    i64::try_from(value).map_err(|_| PostgresTelemetryError::IntegerOverflow { field, value })
}

struct PostgresWriterConnection {
    client: Client,
    _task: JoinHandle<()>,
}

/// Postgres telemetry write error.
#[derive(Debug, thiserror::Error)]
pub enum PostgresTelemetryError {
    /// Connection setup failed.
    #[error(transparent)]
    Connect(#[from] PostgresConnectError),
    /// Connection or query failed.
    #[error("postgres telemetry query failed: {0}")]
    Postgres(#[from] tokio_postgres::Error),
    /// Unsigned event data could not fit into a Postgres BIGINT.
    #[error("postgres telemetry field {field} exceeded BIGINT range: {value}")]
    IntegerOverflow { field: &'static str, value: u64 },
    /// Unsigned sample counter could not fit into a Postgres INTEGER.
    #[error("postgres telemetry sample counter exceeded INTEGER range")]
    SampleCounterOverflow(#[from] std::num::TryFromIntError),
}
