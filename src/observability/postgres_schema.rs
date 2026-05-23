//! Postgres schema and insert helpers for netplay analytics.

use crate::observability::telemetry_event::{
    NetplayPerformanceSample, NetplayTelemetryEvent, NetplayTelemetryRecord,
};

/// Postgres table names used by telemetry and analytics tools.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostgresTableNames {
    pub events: String,
    pub performance_samples: String,
}

/// Split telemetry records into table-specific write batches.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PostgresTelemetryBatch {
    pub events: Vec<NetplayTelemetryEvent>,
    pub performance_samples: Vec<NetplayPerformanceSample>,
}

/// Builds all `CREATE TABLE` and index statements.
pub fn create_table_queries(tables: &PostgresTableNames) -> Vec<String> {
    vec![
        create_events_table_query(&tables.events),
        create_performance_samples_table_query(&tables.performance_samples),
        add_column_query(&tables.events, "timestamp_ms", "BIGINT"),
        add_column_query(&tables.events, "room_id", "UUID"),
        add_column_query(&tables.events, "invite_code", "TEXT"),
        add_column_query(&tables.events, "event_seq", "BIGINT"),
        add_column_query(&tables.events, "room_epoch", "BIGINT"),
        add_column_query(&tables.events, "session_epoch", "BIGINT"),
        add_column_query(&tables.events, "kind", "TEXT"),
        add_column_query(&tables.events, "detail", "TEXT"),
        add_column_query(&tables.performance_samples, "timestamp_ms", "BIGINT"),
        add_column_query(&tables.performance_samples, "room_id", "UUID"),
        add_column_query(&tables.performance_samples, "invite_code", "TEXT"),
        add_column_query(&tables.performance_samples, "event_seq", "BIGINT"),
        add_column_query(&tables.performance_samples, "room_epoch", "BIGINT"),
        add_column_query(&tables.performance_samples, "session_epoch", "BIGINT"),
        add_column_query(&tables.performance_samples, "player_index", "SMALLINT"),
        add_column_query(&tables.performance_samples, "runtime_state", "TEXT"),
        add_column_query(&tables.performance_samples, "local_frame", "BIGINT"),
        add_column_query(&tables.performance_samples, "canonical_frame", "BIGINT"),
        add_column_query(&tables.performance_samples, "released_frame", "BIGINT"),
        add_column_query(&tables.performance_samples, "next_release_frame", "BIGINT"),
        add_column_query(
            &tables.performance_samples,
            "accepted_input_frame",
            "BIGINT",
        ),
        add_column_query(&tables.performance_samples, "frame_delta", "BIGINT"),
        add_column_query(&tables.performance_samples, "round_trip_ms", "INTEGER"),
        add_column_query(&tables.performance_samples, "jitter_ms", "INTEGER"),
        add_column_query(&tables.performance_samples, "prediction_frames", "INTEGER"),
        add_column_query(&tables.performance_samples, "stall_count", "INTEGER"),
        add_column_query(&tables.performance_samples, "catch_up_frames", "INTEGER"),
        add_column_query(&tables.performance_samples, "late_input_frames", "INTEGER"),
        add_column_query(&tables.performance_samples, "audio_underruns", "INTEGER"),
        create_events_session_index_query(&tables.events),
        create_samples_session_index_query(&tables.performance_samples),
    ]
}

/// Groups a mixed telemetry batch by destination table.
pub fn split_batch(batch: &[NetplayTelemetryRecord]) -> PostgresTelemetryBatch {
    let mut grouped = PostgresTelemetryBatch::default();

    for record in batch {
        match record {
            NetplayTelemetryRecord::RoomEvent(event) => grouped.events.push(event.clone()),
            NetplayTelemetryRecord::PerformanceSample(sample) => {
                grouped.performance_samples.push(sample.clone())
            }
        }
    }

    grouped
}

fn create_events_table_query(table: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {} (\
         timestamp_ms BIGINT NOT NULL, \
         room_id UUID NOT NULL, \
         invite_code TEXT NOT NULL, \
         event_seq BIGINT NOT NULL, \
         room_epoch BIGINT NOT NULL, \
         session_epoch BIGINT NOT NULL, \
         kind TEXT NOT NULL, \
         detail TEXT NOT NULL\
         )",
        quote_identifier(table)
    )
}

fn create_performance_samples_table_query(table: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {} (\
         timestamp_ms BIGINT NOT NULL, \
         room_id UUID NOT NULL, \
         invite_code TEXT NOT NULL, \
         event_seq BIGINT NOT NULL, \
         room_epoch BIGINT NOT NULL, \
         session_epoch BIGINT NOT NULL, \
         player_index SMALLINT NOT NULL, \
         runtime_state TEXT NOT NULL, \
         local_frame BIGINT, \
         canonical_frame BIGINT NOT NULL, \
         released_frame BIGINT, \
         next_release_frame BIGINT NOT NULL, \
         accepted_input_frame BIGINT, \
         frame_delta BIGINT, \
         round_trip_ms INTEGER, \
         jitter_ms INTEGER, \
         prediction_frames INTEGER, \
         stall_count INTEGER, \
         catch_up_frames INTEGER, \
         late_input_frames INTEGER, \
         audio_underruns INTEGER\
         )",
        quote_identifier(table)
    )
}

fn create_events_session_index_query(table: &str) -> String {
    format!(
        "CREATE INDEX IF NOT EXISTS {} ON {} \
         (room_id, session_epoch, timestamp_ms, event_seq)",
        quote_identifier(&format!("{table}_session_idx")),
        quote_identifier(table)
    )
}

fn create_samples_session_index_query(table: &str) -> String {
    format!(
        "CREATE INDEX IF NOT EXISTS {} ON {} \
         (room_id, session_epoch, timestamp_ms, player_index)",
        quote_identifier(&format!("{table}_session_idx")),
        quote_identifier(table)
    )
}

fn add_column_query(table: &str, column: &str, data_type: &str) -> String {
    format!(
        "ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {}",
        quote_identifier(table),
        quote_identifier(column),
        data_type
    )
}

/// Quotes a Postgres identifier.
pub fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::telemetry_event::NetplayTelemetryRecord;
    use crate::rooms::RoomId;

    #[test]
    fn postgres_identifiers_are_quoted() {
        assert_eq!(
            quote_identifier("netplay_room_events"),
            "\"netplay_room_events\""
        );
        assert_eq!(quote_identifier("bad\"name"), "\"bad\"\"name\"");
    }

    #[test]
    fn mixed_batch_splits_by_destination_table() {
        let room_id = RoomId::default();
        let batch = vec![
            NetplayTelemetryRecord::RoomEvent(NetplayTelemetryEvent {
                timestamp_ms: 1,
                room_id,
                invite_code: "AB23-CD".to_string(),
                event_seq: 2,
                room_epoch: 3,
                session_epoch: 4,
                kind: "sessionStarted".to_string(),
                detail: "session started".to_string(),
            }),
            NetplayTelemetryRecord::PerformanceSample(NetplayPerformanceSample {
                timestamp_ms: 1,
                room_id,
                invite_code: "AB23-CD".to_string(),
                event_seq: 2,
                room_epoch: 3,
                session_epoch: 4,
                player_index: 0,
                runtime_state: "playing".to_string(),
                local_frame: Some(20),
                canonical_frame: 18,
                released_frame: Some(18),
                next_release_frame: 19,
                accepted_input_frame: Some(20),
                frame_delta: Some(2),
                round_trip_ms: Some(30),
                jitter_ms: Some(3),
                prediction_frames: Some(2),
                stall_count: Some(0),
                catch_up_frames: Some(1),
                late_input_frames: Some(0),
                audio_underruns: Some(0),
            }),
        ];

        let grouped = split_batch(&batch);

        assert_eq!(grouped.events.len(), 1);
        assert_eq!(grouped.performance_samples.len(), 1);
    }
}
