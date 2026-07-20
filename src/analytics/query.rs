//! Postgres queries for the analytics CLI.

use crate::analytics::config::AnalyticsConfig;
use crate::observability::{
    PostgresConnectError, PostgresTableNames, connect_postgres, postgres_schema,
};
use tokio::task::JoinHandle;
use tokio_postgres::{Client, SimpleQueryMessage};

/// Durable netplay session key used by operator reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionKey {
    pub room_id: String,
    pub session_epoch: u64,
    pub invite_code: String,
    pub started_ms: u64,
    pub ended_ms: u64,
}

/// Sanitized room event row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventRow {
    pub timestamp_ms: u64,
    pub room_id: String,
    pub invite_code: String,
    pub event_seq: u64,
    pub room_epoch: u64,
    pub session_epoch: u64,
    pub protocol_version: Option<u16>,
    pub kind: String,
    pub detail: String,
}

/// Sanitized persistent lobby event row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LobbyEventRow {
    pub timestamp_ms: u64,
    pub lobby_id: String,
    pub invite_code: String,
    pub event_seq: u64,
    pub lobby_epoch: u64,
    pub kind: String,
    pub detail: String,
}

/// Sanitized runtime performance sample row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SampleRow {
    pub timestamp_ms: u64,
    pub room_id: String,
    pub invite_code: String,
    pub event_seq: u64,
    pub room_epoch: u64,
    pub session_epoch: u64,
    pub protocol_version: Option<u16>,
    pub player_index: u8,
    pub runtime_state: String,
    pub local_frame: Option<u64>,
    pub canonical_frame: u64,
    pub released_frame: Option<u64>,
    pub next_release_frame: Option<u64>,
    pub accepted_input_frame: Option<u64>,
    pub frame_delta: Option<i64>,
    pub round_trip_ms: Option<u32>,
    pub jitter_ms: Option<u32>,
    pub prediction_frames: Option<u32>,
    pub stall_count: Option<u32>,
    pub catch_up_frames: Option<u32>,
    pub late_input_frames: Option<u32>,
    pub audio_underruns: Option<u32>,
    pub input_resend_frames: Option<u32>,
    pub input_nacks: Option<u32>,
    pub replayed_frames: Option<u32>,
    pub suppressed_audio_frames: Option<u32>,
    pub suppressed_video_frames: Option<u32>,
    pub audio_queue_depth_frames: Option<u32>,
    pub audio_catch_up_events: Option<u32>,
    pub audio_trimmed_frames: Option<u32>,
    pub audio_rebuffer_events: Option<u32>,
    pub audio_max_consecutive_missing_frames: Option<u32>,
    pub audio_queue_min_frames: Option<u32>,
    pub audio_queue_max_frames: Option<u32>,
}

/// Analytics database facade used by CLI commands.
pub struct AnalyticsDb {
    client: Client,
    _task: JoinHandle<()>,
    tables: PostgresTableNames,
}

impl AnalyticsDb {
    /// Opens the configured Postgres analytics database.
    pub async fn connect(config: AnalyticsConfig) -> Result<Self, AnalyticsDbError> {
        let connection = connect_postgres(&config.dsn).await?;

        Ok(Self {
            client: connection.client,
            _task: connection.task,
            tables: config.tables,
        })
    }

    /// Creates all analytics tables and indexes.
    pub async fn apply_schema(&self) -> Result<(), AnalyticsDbError> {
        for query in postgres_schema::create_table_queries(&self.tables) {
            self.client.simple_query(&query).await?;
        }

        Ok(())
    }

    /// Removes synthetic telemetry probe rows from operator reports.
    pub async fn delete_probe_rows(&self) -> Result<(), AnalyticsDbError> {
        for table in [
            &self.tables.events,
            &self.tables.lobby_events,
            &self.tables.performance_samples,
        ] {
            let query = format!(
                "DELETE FROM {} WHERE invite_code = 'PROB-E1'",
                quote_identifier(table)
            );

            self.client.simple_query(&query).await?;
        }

        Ok(())
    }

    /// Returns the latest sessions across all rooms.
    pub async fn recent_sessions(&self, limit: usize) -> Result<Vec<SessionKey>, AnalyticsDbError> {
        self.query_sessions(None, limit).await
    }

    /// Returns latest sessions for one room id.
    pub async fn sessions_for_room(
        &self,
        room_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionKey>, AnalyticsDbError> {
        self.query_sessions(
            Some(format!("WHERE room_id = '{}'", escape_sql(room_id))),
            limit,
        )
        .await
    }

    /// Returns all events for selected sessions.
    pub async fn events_for_sessions(
        &self,
        sessions: &[SessionKey],
    ) -> Result<Vec<EventRow>, AnalyticsDbError> {
        if sessions.is_empty() {
            return Ok(Vec::new());
        }

        let query = format!(
            "SELECT timestamp_ms::text, room_id::text, invite_code, \
             event_seq::text, room_epoch::text, session_epoch::text, \
             COALESCE(protocol_version::text, ''), kind, detail \
             FROM {} WHERE {} ORDER BY timestamp_ms ASC, event_seq ASC",
            quote_identifier(&self.tables.events),
            session_filter(sessions),
        );
        let rows = simple_rows(self.client.simple_query(&query).await?);

        rows.into_iter()
            .map(event_row)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Returns all performance samples for selected sessions.
    pub async fn samples_for_sessions(
        &self,
        sessions: &[SessionKey],
    ) -> Result<Vec<SampleRow>, AnalyticsDbError> {
        if sessions.is_empty() {
            return Ok(Vec::new());
        }

        let query = format!(
            "SELECT timestamp_ms::text, room_id::text, invite_code, \
             event_seq::text, room_epoch::text, session_epoch::text, \
             COALESCE(protocol_version::text, ''), player_index::text, runtime_state, \
             COALESCE(local_frame::text, ''), \
             canonical_frame::text, COALESCE(released_frame::text, ''), \
             COALESCE(next_release_frame::text, ''), \
             COALESCE(accepted_input_frame::text, ''), \
             COALESCE(frame_delta::text, ''), \
             COALESCE(round_trip_ms::text, ''), COALESCE(jitter_ms::text, ''), \
             COALESCE(prediction_frames::text, ''), COALESCE(stall_count::text, ''), \
             COALESCE(catch_up_frames::text, ''), COALESCE(late_input_frames::text, ''), \
             COALESCE(audio_underruns::text, ''), COALESCE(input_resend_frames::text, ''), \
             COALESCE(input_nacks::text, ''), COALESCE(replayed_frames::text, ''), \
             COALESCE(suppressed_audio_frames::text, ''), \
             COALESCE(suppressed_video_frames::text, ''), \
             COALESCE(audio_queue_depth_frames::text, ''), \
             COALESCE(audio_catch_up_events::text, ''), \
             COALESCE(audio_trimmed_frames::text, ''), \
             COALESCE(audio_rebuffer_events::text, ''), \
             COALESCE(audio_max_consecutive_missing_frames::text, ''), \
             COALESCE(audio_queue_min_frames::text, ''), \
             COALESCE(audio_queue_max_frames::text, '') \
             FROM {} WHERE {} ORDER BY timestamp_ms ASC, player_index ASC",
            quote_identifier(&self.tables.performance_samples),
            session_filter(sessions),
        );
        let rows = simple_rows(self.client.simple_query(&query).await?);

        rows.into_iter()
            .map(sample_row)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Returns recent lobby events across all active lobby telemetry rows.
    pub async fn recent_lobby_events(
        &self,
        limit: usize,
    ) -> Result<Vec<LobbyEventRow>, AnalyticsDbError> {
        self.query_lobby_events(None, limit).await
    }

    /// Returns recent lobby events for one invite code.
    pub async fn lobby_events_for_invite(
        &self,
        invite_code: &str,
        limit: usize,
    ) -> Result<Vec<LobbyEventRow>, AnalyticsDbError> {
        self.query_lobby_events(
            Some(format!("WHERE invite_code = '{}'", escape_sql(invite_code))),
            limit,
        )
        .await
    }

    async fn query_sessions(
        &self,
        where_clause: Option<String>,
        limit: usize,
    ) -> Result<Vec<SessionKey>, AnalyticsDbError> {
        let where_clause = where_clause.unwrap_or_default();
        let query = format!(
            "SELECT room_id::text, session_epoch::text, \
             (array_agg(invite_code ORDER BY timestamp_ms DESC))[1], \
             min(timestamp_ms)::text, max(timestamp_ms)::text \
             FROM (\
               SELECT room_id, invite_code, session_epoch, timestamp_ms FROM {} \
               UNION ALL \
               SELECT room_id, invite_code, session_epoch, timestamp_ms FROM {}\
             ) sessions {where_clause} \
             GROUP BY room_id, session_epoch \
             ORDER BY max(timestamp_ms) DESC \
             LIMIT {}",
            quote_identifier(&self.tables.events),
            quote_identifier(&self.tables.performance_samples),
            limit.clamp(1, 500),
        );
        let rows = simple_rows(self.client.simple_query(&query).await?);

        rows.into_iter()
            .map(session_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn query_lobby_events(
        &self,
        where_clause: Option<String>,
        limit: usize,
    ) -> Result<Vec<LobbyEventRow>, AnalyticsDbError> {
        let query = format!(
            "SELECT timestamp_ms::text, lobby_id::text, invite_code, \
             event_seq::text, lobby_epoch::text, kind, detail \
             FROM {} {} ORDER BY timestamp_ms DESC, event_seq DESC LIMIT {}",
            quote_identifier(&self.tables.lobby_events),
            where_clause.unwrap_or_default(),
            limit.clamp(1, 500),
        );
        let rows = simple_rows(self.client.simple_query(&query).await?);

        rows.into_iter()
            .map(lobby_event_row)
            .collect::<Result<Vec<_>, _>>()
    }
}

fn session_row(row: tokio_postgres::SimpleQueryRow) -> Result<SessionKey, AnalyticsDbError> {
    Ok(SessionKey {
        room_id: required_cell(&row, 0, "room_id")?.to_string(),
        session_epoch: parse_u64(required_cell(&row, 1, "session_epoch")?, "session_epoch")?,
        invite_code: required_cell(&row, 2, "invite_code")?.to_string(),
        started_ms: parse_u64(required_cell(&row, 3, "started_ms")?, "started_ms")?,
        ended_ms: parse_u64(required_cell(&row, 4, "ended_ms")?, "ended_ms")?,
    })
}

fn event_row(row: tokio_postgres::SimpleQueryRow) -> Result<EventRow, AnalyticsDbError> {
    Ok(EventRow {
        timestamp_ms: parse_u64(required_cell(&row, 0, "timestamp_ms")?, "timestamp_ms")?,
        room_id: required_cell(&row, 1, "room_id")?.to_string(),
        invite_code: required_cell(&row, 2, "invite_code")?.to_string(),
        event_seq: parse_u64(required_cell(&row, 3, "event_seq")?, "event_seq")?,
        room_epoch: parse_u64(required_cell(&row, 4, "room_epoch")?, "room_epoch")?,
        session_epoch: parse_u64(required_cell(&row, 5, "session_epoch")?, "session_epoch")?,
        protocol_version: parse_optional_u16(
            required_cell(&row, 6, "protocol_version")?,
            "protocol_version",
        )?,
        kind: required_cell(&row, 7, "kind")?.to_string(),
        detail: required_cell(&row, 8, "detail")?.to_string(),
    })
}

fn lobby_event_row(row: tokio_postgres::SimpleQueryRow) -> Result<LobbyEventRow, AnalyticsDbError> {
    Ok(LobbyEventRow {
        timestamp_ms: parse_u64(required_cell(&row, 0, "timestamp_ms")?, "timestamp_ms")?,
        lobby_id: required_cell(&row, 1, "lobby_id")?.to_string(),
        invite_code: required_cell(&row, 2, "invite_code")?.to_string(),
        event_seq: parse_u64(required_cell(&row, 3, "event_seq")?, "event_seq")?,
        lobby_epoch: parse_u64(required_cell(&row, 4, "lobby_epoch")?, "lobby_epoch")?,
        kind: required_cell(&row, 5, "kind")?.to_string(),
        detail: required_cell(&row, 6, "detail")?.to_string(),
    })
}

fn sample_row(row: tokio_postgres::SimpleQueryRow) -> Result<SampleRow, AnalyticsDbError> {
    Ok(SampleRow {
        timestamp_ms: parse_u64(required_cell(&row, 0, "timestamp_ms")?, "timestamp_ms")?,
        room_id: required_cell(&row, 1, "room_id")?.to_string(),
        invite_code: required_cell(&row, 2, "invite_code")?.to_string(),
        event_seq: parse_u64(required_cell(&row, 3, "event_seq")?, "event_seq")?,
        room_epoch: parse_u64(required_cell(&row, 4, "room_epoch")?, "room_epoch")?,
        session_epoch: parse_u64(required_cell(&row, 5, "session_epoch")?, "session_epoch")?,
        protocol_version: parse_optional_u16(
            required_cell(&row, 6, "protocol_version")?,
            "protocol_version",
        )?,
        player_index: parse_u8(required_cell(&row, 7, "player_index")?, "player_index")?,
        runtime_state: required_cell(&row, 8, "runtime_state")?.to_string(),
        local_frame: parse_optional_u64(required_cell(&row, 9, "local_frame")?, "local_frame")?,
        canonical_frame: parse_u64(
            required_cell(&row, 10, "canonical_frame")?,
            "canonical_frame",
        )?,
        released_frame: parse_optional_u64(
            required_cell(&row, 11, "released_frame")?,
            "released_frame",
        )?,
        next_release_frame: parse_optional_u64(
            required_cell(&row, 12, "next_release_frame")?,
            "next_release_frame",
        )?,
        accepted_input_frame: parse_optional_u64(
            required_cell(&row, 13, "accepted_input_frame")?,
            "accepted_input_frame",
        )?,
        frame_delta: parse_optional_i64(required_cell(&row, 14, "frame_delta")?, "frame_delta")?,
        round_trip_ms: parse_optional_u32(
            required_cell(&row, 15, "round_trip_ms")?,
            "round_trip_ms",
        )?,
        jitter_ms: parse_optional_u32(required_cell(&row, 16, "jitter_ms")?, "jitter_ms")?,
        prediction_frames: parse_optional_u32(
            required_cell(&row, 17, "prediction_frames")?,
            "prediction_frames",
        )?,
        stall_count: parse_optional_u32(required_cell(&row, 18, "stall_count")?, "stall_count")?,
        catch_up_frames: parse_optional_u32(
            required_cell(&row, 19, "catch_up_frames")?,
            "catch_up_frames",
        )?,
        late_input_frames: parse_optional_u32(
            required_cell(&row, 20, "late_input_frames")?,
            "late_input_frames",
        )?,
        audio_underruns: parse_optional_u32(
            required_cell(&row, 21, "audio_underruns")?,
            "audio_underruns",
        )?,
        input_resend_frames: parse_optional_u32(
            required_cell(&row, 22, "input_resend_frames")?,
            "input_resend_frames",
        )?,
        input_nacks: parse_optional_u32(required_cell(&row, 23, "input_nacks")?, "input_nacks")?,
        replayed_frames: parse_optional_u32(
            required_cell(&row, 24, "replayed_frames")?,
            "replayed_frames",
        )?,
        suppressed_audio_frames: parse_optional_u32(
            required_cell(&row, 25, "suppressed_audio_frames")?,
            "suppressed_audio_frames",
        )?,
        suppressed_video_frames: parse_optional_u32(
            required_cell(&row, 26, "suppressed_video_frames")?,
            "suppressed_video_frames",
        )?,
        audio_queue_depth_frames: parse_optional_u32(
            required_cell(&row, 27, "audio_queue_depth_frames")?,
            "audio_queue_depth_frames",
        )?,
        audio_catch_up_events: parse_optional_u32(
            required_cell(&row, 28, "audio_catch_up_events")?,
            "audio_catch_up_events",
        )?,
        audio_trimmed_frames: parse_optional_u32(
            required_cell(&row, 29, "audio_trimmed_frames")?,
            "audio_trimmed_frames",
        )?,
        audio_rebuffer_events: parse_optional_u32(
            required_cell(&row, 30, "audio_rebuffer_events")?,
            "audio_rebuffer_events",
        )?,
        audio_max_consecutive_missing_frames: parse_optional_u32(
            required_cell(&row, 31, "audio_max_consecutive_missing_frames")?,
            "audio_max_consecutive_missing_frames",
        )?,
        audio_queue_min_frames: parse_optional_u32(
            required_cell(&row, 32, "audio_queue_min_frames")?,
            "audio_queue_min_frames",
        )?,
        audio_queue_max_frames: parse_optional_u32(
            required_cell(&row, 33, "audio_queue_max_frames")?,
            "audio_queue_max_frames",
        )?,
    })
}

fn session_filter(sessions: &[SessionKey]) -> String {
    let values = sessions
        .iter()
        .map(|session| {
            format!(
                "('{}'::uuid, {})",
                escape_sql(&session.room_id),
                session.session_epoch
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("(room_id, session_epoch) IN ({values})")
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

pub fn quote_identifier(value: &str) -> String {
    postgres_schema::quote_identifier(value)
}

fn escape_sql(value: &str) -> String {
    value.replace('\'', "''")
}

fn required_cell<'a>(
    row: &'a tokio_postgres::SimpleQueryRow,
    index: usize,
    field: &'static str,
) -> Result<&'a str, AnalyticsDbError> {
    row.get(index)
        .ok_or(AnalyticsDbError::MalformedRow { field })
}

fn parse_u64(value: &str, field: &'static str) -> Result<u64, AnalyticsDbError> {
    parse_required(value, field)
}

fn parse_u8(value: &str, field: &'static str) -> Result<u8, AnalyticsDbError> {
    parse_required(value, field)
}

fn parse_optional_u64(value: &str, field: &'static str) -> Result<Option<u64>, AnalyticsDbError> {
    parse_optional(value, field)
}

fn parse_optional_u32(value: &str, field: &'static str) -> Result<Option<u32>, AnalyticsDbError> {
    parse_optional(value, field)
}

fn parse_optional_u16(value: &str, field: &'static str) -> Result<Option<u16>, AnalyticsDbError> {
    parse_optional(value, field)
}

fn parse_optional_i64(value: &str, field: &'static str) -> Result<Option<i64>, AnalyticsDbError> {
    parse_optional(value, field)
}

fn parse_required<T: std::str::FromStr>(
    value: &str,
    field: &'static str,
) -> Result<T, AnalyticsDbError> {
    value
        .parse()
        .map_err(|_| AnalyticsDbError::MalformedNumber {
            field,
            value: value.to_string(),
        })
}

fn parse_optional<T: std::str::FromStr>(
    value: &str,
    field: &'static str,
) -> Result<Option<T>, AnalyticsDbError> {
    if value.is_empty() {
        return Ok(None);
    }

    parse_required(value, field).map(Some)
}

/// Analytics database failure.
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsDbError {
    #[error(transparent)]
    Connect(#[from] PostgresConnectError),
    #[error("analytics query failed")]
    Postgres(#[from] tokio_postgres::Error),
    #[error("analytics query returned a row without field {field}")]
    MalformedRow { field: &'static str },
    #[error("analytics query returned invalid number for {field}: {value}")]
    MalformedNumber { field: &'static str, value: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_identifier_escapes_double_quotes() {
        assert_eq!(quote_identifier("room\"events"), "\"room\"\"events\"");
    }

    #[test]
    fn session_filter_uses_postgres_uuid_casts() {
        let filter = session_filter(&[SessionKey {
            room_id: "00000000-0000-0000-0000-000000000000".to_string(),
            session_epoch: 2,
            invite_code: "AB23-CD".to_string(),
            started_ms: 0,
            ended_ms: 0,
        }]);

        assert!(filter.contains("'00000000-0000-0000-0000-000000000000'::uuid"));
        assert!(!filter.contains("toUUID"));
    }

    #[test]
    fn numeric_parsing_rejects_bad_values_instead_of_defaulting() {
        let error = parse_u64("not-a-number", "timestamp_ms").expect_err("bad value");

        assert!(matches!(
            error,
            AnalyticsDbError::MalformedNumber {
                field: "timestamp_ms",
                ..
            }
        ));
    }

    #[test]
    fn optional_numeric_parsing_accepts_empty_cells_only() {
        assert_eq!(
            parse_optional_u32("", "round_trip_ms").expect("empty optional"),
            None
        );
        assert!(parse_optional_u32("bad", "round_trip_ms").is_err());
    }
}
