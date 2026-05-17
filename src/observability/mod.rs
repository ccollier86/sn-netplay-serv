//! Production observability helpers.
//!
//! This module owns process metrics and tracing setup. It does not know room
//! mutation rules or HTTP authentication details.

mod metrics;
mod tracing_setup;

pub use metrics::{InMemoryMetrics, MetricsRecorder, MetricsSnapshot};
pub use tracing_setup::init_tracing;
