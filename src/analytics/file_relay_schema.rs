//! Operator-side schema helpers for file-relay telemetry.

/// Builds all schema statements for the shared file-relay transfer table.
pub fn create_table_queries(table: &str) -> Vec<String> {
    vec![
        create_transfer_events_table_query(table),
        add_column_query(table, "timestamp_ms", "BIGINT"),
        add_column_query(table, "transfer_id", "TEXT"),
        add_column_query(table, "room_id", "TEXT"),
        add_column_query(table, "sender_player_id", "TEXT"),
        add_column_query(table, "receiver_player_id", "TEXT"),
        add_column_query(table, "kind", "TEXT"),
        add_column_query(table, "phase", "TEXT"),
        add_column_query(table, "status", "TEXT"),
        add_column_query(table, "size_bytes", "BIGINT"),
        add_column_query(table, "uploaded_bytes", "BIGINT"),
        add_column_query(table, "downloaded_bytes", "BIGINT"),
        add_column_query(table, "chunk_index", "BIGINT"),
        add_column_query(table, "chunk_count", "BIGINT"),
        add_column_query(table, "detail", "TEXT"),
        create_time_index_query(table),
        create_transfer_index_query(table),
        create_room_index_query(table),
    ]
}

fn create_transfer_events_table_query(table: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {} (\
         timestamp_ms BIGINT NOT NULL, \
         transfer_id TEXT NOT NULL, \
         room_id TEXT NOT NULL, \
         sender_player_id TEXT NOT NULL, \
         receiver_player_id TEXT NOT NULL, \
         kind TEXT NOT NULL, \
         phase TEXT NOT NULL, \
         status TEXT NOT NULL, \
         size_bytes BIGINT NOT NULL, \
         uploaded_bytes BIGINT NOT NULL, \
         downloaded_bytes BIGINT NOT NULL, \
         chunk_index BIGINT, \
         chunk_count BIGINT NOT NULL, \
         detail TEXT NOT NULL\
         )",
        quote_identifier(table)
    )
}

fn create_time_index_query(table: &str) -> String {
    format!(
        "CREATE INDEX IF NOT EXISTS {} ON {} (timestamp_ms DESC)",
        quote_identifier(&format!("{table}_time_idx")),
        quote_identifier(table)
    )
}

fn create_transfer_index_query(table: &str) -> String {
    format!(
        "CREATE INDEX IF NOT EXISTS {} ON {} (transfer_id, timestamp_ms)",
        quote_identifier(&format!("{table}_transfer_idx")),
        quote_identifier(table)
    )
}

fn create_room_index_query(table: &str) -> String {
    format!(
        "CREATE INDEX IF NOT EXISTS {} ON {} (room_id, timestamp_ms)",
        quote_identifier(&format!("{table}_room_idx")),
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

pub fn quote_identifier(value: &str) -> String {
    crate::observability::postgres_schema::quote_identifier(value)
}
