CREATE TABLE IF NOT EXISTS netplay_room_events
(
  timestamp_ms UInt64,
  room_id UUID,
  invite_code String,
  event_seq UInt64,
  room_epoch UInt64,
  session_epoch UInt64,
  kind LowCardinality(String),
  detail String
)
ENGINE = MergeTree
ORDER BY (timestamp_ms, room_id, session_epoch, event_seq);

CREATE TABLE IF NOT EXISTS netplay_performance_samples
(
  timestamp_ms UInt64,
  room_id UUID,
  invite_code String,
  event_seq UInt64,
  room_epoch UInt64,
  session_epoch UInt64,
  player_index UInt8,
  runtime_state LowCardinality(String),
  local_frame Nullable(UInt64),
  canonical_frame UInt64,
  released_frame Nullable(UInt64),
  next_release_frame UInt64,
  accepted_input_frame Nullable(UInt64),
  frame_delta Nullable(Int64),
  round_trip_ms Nullable(UInt32),
  jitter_ms Nullable(UInt32),
  prediction_frames Nullable(UInt32),
  stall_count Nullable(UInt32),
  catch_up_frames Nullable(UInt32),
  late_input_frames Nullable(UInt32),
  audio_underruns Nullable(UInt32),
  input_resend_frames Nullable(UInt32),
  input_nacks Nullable(UInt32),
  replayed_frames Nullable(UInt32),
  suppressed_audio_frames Nullable(UInt32),
  suppressed_video_frames Nullable(UInt32),
  audio_queue_depth_frames Nullable(UInt32),
  audio_catch_up_events Nullable(UInt32),
  audio_trimmed_frames Nullable(UInt32),
  audio_rebuffer_events Nullable(UInt32),
  audio_max_consecutive_missing_frames Nullable(UInt32),
  audio_queue_min_frames Nullable(UInt32),
  audio_queue_max_frames Nullable(UInt32)
)
ENGINE = MergeTree
ORDER BY (timestamp_ms, room_id, session_epoch, player_index);
