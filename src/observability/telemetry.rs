//! Async durable telemetry drain.
//!
//! Gameplay code only performs a bounded `try_send`. External storage writes
//! happen in this background task and are allowed to fail without affecting
//! active rooms.

use crate::config::{TelemetryConfig, TelemetrySinkConfig};
use crate::lobbies::{LobbyDebugEvent, LobbyDebugEventSink, NoopLobbyDebugEventSink};
use crate::observability::MetricsRecorder;
use crate::observability::postgres_telemetry_writer::PostgresTelemetryWriter;
use crate::observability::telemetry_event::NetplayTelemetryRecord;
use crate::rooms::{
    NoopRoomDebugEventSink, RoomDebugEvent, RoomDebugEventSink, RoomPerformanceSample,
};
use std::sync::Arc;
use tokio::sync::mpsc::{self, error::TrySendError};
use tokio::task::JoinHandle;
use tokio::time::{Duration, MissedTickBehavior};
use tracing::warn;

/// Room sink, lobby sink, and optional background drain task returned at startup.
pub type TelemetrySinkHandles = (
    Arc<dyn RoomDebugEventSink>,
    Arc<dyn LobbyDebugEventSink>,
    Option<JoinHandle<()>>,
);

/// Creates the configured room-event sink and optional background drain task.
pub fn spawn_telemetry_sink(
    config: TelemetryConfig,
    metrics: Arc<dyn MetricsRecorder>,
) -> TelemetrySinkHandles {
    match config.sink {
        TelemetrySinkConfig::Disabled => (
            Arc::new(NoopRoomDebugEventSink),
            Arc::new(NoopLobbyDebugEventSink),
            None,
        ),
        TelemetrySinkConfig::Postgres(postgres) => {
            let (sender, receiver) = mpsc::channel(config.queue_capacity);
            let sink = Arc::new(BoundedTelemetrySink {
                sender,
                metrics: metrics.clone(),
            });
            let lobby_sink = sink.clone();
            let writer = TelemetryWriter::Postgres(PostgresTelemetryWriter::new(postgres));
            let task = tokio::spawn(run_telemetry_drain(
                receiver,
                writer,
                config.batch_size,
                config.flush_interval,
                metrics,
            ));

            (sink, lobby_sink, Some(task))
        }
    }
}

enum TelemetryWriter {
    Postgres(PostgresTelemetryWriter),
}

impl TelemetryWriter {
    async fn write_batch(
        &mut self,
        batch: &[NetplayTelemetryRecord],
    ) -> Result<(), TelemetryWriterError> {
        match self {
            Self::Postgres(writer) => writer
                .write_batch(batch)
                .await
                .map_err(|error| TelemetryWriterError::Postgres(error.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum TelemetryWriterError {
    #[error("postgres telemetry failed: {0}")]
    Postgres(String),
}

struct BoundedTelemetrySink {
    sender: mpsc::Sender<NetplayTelemetryRecord>,
    metrics: Arc<dyn MetricsRecorder>,
}

impl RoomDebugEventSink for BoundedTelemetrySink {
    fn record(&self, event: RoomDebugEvent) {
        self.try_record(NetplayTelemetryRecord::RoomEvent(event.into()));
    }

    fn record_performance_sample(&self, sample: RoomPerformanceSample) {
        self.try_record(NetplayTelemetryRecord::PerformanceSample(sample.into()));
    }
}

impl LobbyDebugEventSink for BoundedTelemetrySink {
    fn record_lobby_event(&self, event: LobbyDebugEvent) {
        self.try_record(NetplayTelemetryRecord::LobbyEvent(event.into()));
    }
}

impl BoundedTelemetrySink {
    fn try_record(&self, record: NetplayTelemetryRecord) {
        match self.sender.try_send(record) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {
                self.metrics.record_telemetry_dropped(1);
            }
        }
    }
}

async fn run_telemetry_drain(
    mut receiver: mpsc::Receiver<NetplayTelemetryRecord>,
    mut writer: TelemetryWriter,
    batch_size: usize,
    flush_interval: Duration,
    metrics: Arc<dyn MetricsRecorder>,
) {
    let mut ticker = tokio::time::interval(flush_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut batch = Vec::with_capacity(batch_size);

    loop {
        tokio::select! {
            Some(event) = receiver.recv() => {
                batch.push(event);

                if batch.len() >= batch_size {
                    flush_batch(&mut writer, &mut batch, metrics.as_ref()).await;
                }
            }
            _ = ticker.tick() => {
                flush_batch(&mut writer, &mut batch, metrics.as_ref()).await;
            }
            else => break,
        }
    }

    flush_batch(&mut writer, &mut batch, metrics.as_ref()).await;
}

async fn flush_batch(
    writer: &mut TelemetryWriter,
    batch: &mut Vec<NetplayTelemetryRecord>,
    metrics: &dyn MetricsRecorder,
) {
    if batch.is_empty() {
        return;
    }

    if let Err(error) = writer.write_batch(batch).await {
        warn!(%error, events = batch.len(), "durable telemetry batch failed");
        metrics.record_telemetry_write_failed(batch.len() as u64);
    }

    batch.clear();
}
