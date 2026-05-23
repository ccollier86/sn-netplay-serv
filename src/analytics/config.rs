//! Analytics CLI configuration.

use crate::observability::{PostgresDsn, PostgresTableNames};
use std::env;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyticsConfig {
    pub dsn: PostgresDsn,
    pub tables: PostgresTableNames,
}

impl AnalyticsConfig {
    pub fn from_env() -> Result<Self, AnalyticsConfigError> {
        let dsn = PostgresDsn::parse(
            env::var("SB_NETPLAY_POSTGRES_URL").map_err(|_| AnalyticsConfigError::MissingDsn)?,
        )
        .map_err(|_| AnalyticsConfigError::InvalidDsn)?;
        let legacy_events_table = optional_env("SB_NETPLAY_POSTGRES_TABLE");
        let tables = PostgresTableNames {
            events: optional_env("SB_NETPLAY_POSTGRES_EVENTS_TABLE")
                .or(legacy_events_table)
                .unwrap_or_else(|| "netplay_room_events".to_string()),
            performance_samples: optional_env("SB_NETPLAY_POSTGRES_PERFORMANCE_TABLE")
                .unwrap_or_else(|| "netplay_performance_samples".to_string()),
        };

        Ok(Self { dsn, tables })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnalyticsConfigError {
    #[error("SB_NETPLAY_POSTGRES_URL is required")]
    MissingDsn,
    #[error(
        "SB_NETPLAY_POSTGRES_URL must be postgres://user:pass@host:port/database with optional sslmode=require|prefer|disable|verify-ca|verify-full"
    )]
    InvalidDsn,
}

fn optional_env(name: &'static str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}
