//! Postgres DSN parsing for durable netplay telemetry.
//!
//! The DSN may contain credentials, so this module redacts it from debug output
//! while preserving the original value used by `tokio-postgres`.

use std::fmt;
use std::str::FromStr;
use url::Url;

/// Parsed Postgres telemetry DSN.
#[derive(Clone, Eq, PartialEq)]
pub struct PostgresDsn {
    value: String,
    database: String,
    tls_mode: PostgresTlsMode,
}

impl PostgresDsn {
    /// Parses a `postgres://user:pass@host:port/database` telemetry DSN.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, PostgresDsnError> {
        let value = value.as_ref().trim();
        let url = Url::parse(value).map_err(|_| PostgresDsnError::Invalid)?;

        if !matches!(url.scheme(), "postgres" | "postgresql") {
            return Err(PostgresDsnError::UnsupportedScheme);
        }

        let database = url.path().trim_start_matches('/').trim().to_string();
        if database.is_empty() {
            return Err(PostgresDsnError::MissingDatabase);
        }

        Ok(Self {
            database,
            tls_mode: postgres_tls_mode(&url)?,
            value: value.to_string(),
        })
    }

    /// Returns the original PostgreSQL connection string.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Database selected by the DSN path.
    pub fn database(&self) -> &str {
        &self.database
    }

    /// TLS behavior requested by the DSN `sslmode` value.
    pub fn tls_mode(&self) -> PostgresTlsMode {
        self.tls_mode
    }
}

impl fmt::Debug for PostgresDsn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresDsn")
            .field("database", &self.database)
            .field("tls_mode", &self.tls_mode)
            .field("value", &"<redacted>")
            .finish()
    }
}

impl FromStr for PostgresDsn {
    type Err = PostgresDsnError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// DSN validation error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PostgresDsnError {
    /// DSN is not valid URL syntax.
    #[error("postgres telemetry DSN is invalid")]
    Invalid,
    /// DSN must use the Postgres scheme expected by deployment env.
    #[error("postgres telemetry DSN must use postgres:// or postgresql://")]
    UnsupportedScheme,
    /// DSN must include a database path.
    #[error("postgres telemetry DSN must include a database")]
    MissingDatabase,
    /// The optional sslmode query string was not supported.
    #[error(
        "postgres telemetry DSN sslmode must be require, prefer, disable, verify-ca, or verify-full"
    )]
    InvalidSslMode,
}

/// TLS policy for Postgres connections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostgresTlsMode {
    /// Require encrypted transport without certificate-chain validation.
    Require,
    /// Try encrypted transport first, then fall back to plaintext.
    Prefer,
    /// Use plaintext only.
    Disable,
    /// Require encrypted transport with platform-native certificate validation.
    Verify,
}

fn postgres_tls_mode(url: &Url) -> Result<PostgresTlsMode, PostgresDsnError> {
    let Some((_, value)) = url.query_pairs().find(|(key, _)| key == "sslmode") else {
        return Ok(PostgresTlsMode::Prefer);
    };

    match value.as_ref() {
        "require" => Ok(PostgresTlsMode::Require),
        "prefer" => Ok(PostgresTlsMode::Prefer),
        "disable" => Ok(PostgresTlsMode::Disable),
        "verify-ca" | "verify-full" => Ok(PostgresTlsMode::Verify),
        _ => Err(PostgresDsnError::InvalidSslMode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_postgres_dsn_without_exposing_password_in_debug() {
        let dsn =
            PostgresDsn::parse("postgres://user:secret@example.com:5432/netplay?sslmode=require")
                .expect("dsn");

        assert_eq!(dsn.database(), "netplay");
        assert_eq!(dsn.tls_mode(), PostgresTlsMode::Require);
        assert!(dsn.value().contains("secret"));
        assert!(!format!("{dsn:?}").contains("secret"));
    }

    #[test]
    fn rejects_non_postgres_scheme() {
        assert_eq!(
            PostgresDsn::parse("clickhouse://user:secret@example.com:5432/default")
                .expect_err("invalid scheme"),
            PostgresDsnError::UnsupportedScheme
        );
    }

    #[test]
    fn rejects_unknown_sslmode() {
        assert_eq!(
            PostgresDsn::parse("postgres://user:secret@example.com:5432/default?sslmode=maybe")
                .expect_err("invalid sslmode"),
            PostgresDsnError::InvalidSslMode
        );
    }
}
