//! Process tracing configuration.
//!
//! The binary calls this once after environment configuration is loaded.

use crate::config::{LogConfig, LogFormat};
use tracing_subscriber::EnvFilter;

/// Initializes global tracing from config and `RUST_LOG`.
pub fn init_tracing(config: LogConfig) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sb_netplay_serv=info,tower_http=info"));

    match config.format {
        LogFormat::Compact => tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .compact()
            .init(),
        LogFormat::Json => tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .json()
            .init(),
    }
}
