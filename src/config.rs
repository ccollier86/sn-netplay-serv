//! Environment configuration for the netplay relay.
//!
//! This module owns parsing environment variables into typed configuration. It
//! does not construct services or start listeners.

use std::env;
use std::fmt;
use std::net::{AddrParseError, SocketAddr};

use crate::file_relay::FileRelayConfig;
use crate::observability::{PostgresDsn, PostgresTableNames};
use crate::rate_limit::RateLimitPolicy;
use crate::rooms::RoomRecoveryConfig;

/// Runtime settings required to start the relay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    /// TCP address used by the HTTP/WebSocket server.
    pub bind_addr: SocketAddr,
    /// Full metadata-service URL used for netplay client authorization.
    pub authorize_url: String,
    /// Server-to-server secret sent to the license authority.
    pub license_internal_secret: String,
    /// Optional bearer token required by internal observability endpoints.
    pub admin_token: Option<String>,
    /// Whether request identity may use proxy forwarding headers.
    pub trust_proxy_headers: bool,
    /// Per-action request rate limits.
    pub rate_limits: RateLimitPolicy,
    /// In-memory room recovery and heartbeat timing.
    pub recovery: RoomRecoveryConfig,
    /// How long a lobby may remain without meaningful user or gameplay activity.
    pub lobby_idle: std::time::Duration,
    /// Logging output settings.
    pub log: LogConfig,
    /// Optional durable analytics sink.
    pub telemetry: TelemetryConfig,
    /// Optional trusted voice broker used for LiveKit room orchestration.
    pub voice: VoiceConfig,
    /// Optional trusted file relay used for temporary transfer tickets.
    pub file_relay: FileRelayConfig,
}

impl ServerConfig {
    /// Reads configuration from process environment variables.
    ///
    /// `SB_NETPLAY_BIND_ADDR` defaults to `127.0.0.1:8077`. License authority
    /// URL and internal secret are required so production cannot silently run
    /// with an allow-all verifier.
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind_addr = env::var("SB_NETPLAY_BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8077".to_string())
            .parse()?;
        let authorize_url = required_env_with_fallbacks(
            "SB_NETPLAY_AUTHORIZE_URL",
            &[
                "SB_NETPLAY_DESKTOP_AUTHORIZE_URL",
                "SB_NETPLAY_LICENSE_VERIFY_URL",
            ],
        )?;
        let license_internal_secret = required_env("SB_NETPLAY_LICENSE_INTERNAL_SECRET")?;
        let admin_token = optional_env("SB_NETPLAY_ADMIN_TOKEN")?;
        let trust_proxy_headers = optional_bool_env("SB_NETPLAY_TRUST_PROXY_HEADERS", false)?;
        let rate_limits = RateLimitPolicy {
            create_room_per_minute: optional_u32_env("SB_NETPLAY_RATE_CREATE_ROOM_PER_MINUTE", 12)?,
            websocket_join_per_minute: optional_u32_env("SB_NETPLAY_RATE_WS_JOIN_PER_MINUTE", 30)?,
            room_status_per_minute: optional_u32_env(
                "SB_NETPLAY_RATE_ROOM_STATUS_PER_MINUTE",
                120,
            )?,
        };
        let recovery = RoomRecoveryConfig {
            reconnect_grace: optional_duration_seconds_env(
                "SB_NETPLAY_RECONNECT_GRACE_SECONDS",
                90,
            )?,
            heartbeat_stale: optional_duration_seconds_env(
                "SB_NETPLAY_HEARTBEAT_STALE_SECONDS",
                15,
            )?,
            heartbeat_disconnect: optional_duration_seconds_env(
                "SB_NETPLAY_HEARTBEAT_DISCONNECT_SECONDS",
                30,
            )?,
            room_idle: optional_duration_seconds_env("SB_NETPLAY_ROOM_IDLE_SECONDS", 300)?,
        };
        let lobby_idle = optional_duration_seconds_env("SB_NETPLAY_LOBBY_IDLE_SECONDS", 3600)?;
        let log = LogConfig {
            format: optional_log_format_env("SB_NETPLAY_LOG_FORMAT", LogFormat::Compact)?,
        };
        let telemetry = TelemetryConfig::from_env()?;
        let voice = VoiceConfig::from_env()?;
        let file_relay = FileRelayConfig::from_env()?;

        Ok(Self {
            bind_addr,
            authorize_url,
            license_internal_secret,
            admin_token,
            trust_proxy_headers,
            rate_limits,
            recovery,
            lobby_idle,
            log,
            telemetry,
            voice,
            file_relay,
        })
    }
}

/// Optional voice broker integration configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceConfig {
    /// Selected voice broker backend.
    pub broker: VoiceBrokerConfig,
}

impl VoiceConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let broker_url = optional_env("SB_NETPLAY_VOICE_BROKER_URL")?;
        let broker_token = optional_env("SB_NETPLAY_VOICE_BROKER_TOKEN")?;
        let request_timeout =
            optional_duration_millis_env("SB_NETPLAY_VOICE_BROKER_TIMEOUT_MS", 2500)?;

        let broker = match (broker_url, broker_token) {
            (None, None) => VoiceBrokerConfig::Disabled,
            (Some(base_url), Some(bearer_token)) => {
                VoiceBrokerConfig::Http(HttpVoiceBrokerConfig {
                    base_url,
                    bearer_token,
                    request_timeout,
                })
            }
            (Some(_), None) => {
                return Err(ConfigError::MissingEnv("SB_NETPLAY_VOICE_BROKER_TOKEN"));
            }
            (None, Some(_)) => return Err(ConfigError::MissingEnv("SB_NETPLAY_VOICE_BROKER_URL")),
        };

        Ok(Self { broker })
    }
}

/// Supported voice broker backends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoiceBrokerConfig {
    /// Voice broker integration is disabled.
    Disabled,
    /// HTTP broker compatible with `sb-webrtc`.
    Http(HttpVoiceBrokerConfig),
}

/// HTTP voice broker configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct HttpVoiceBrokerConfig {
    /// Base URL for the trusted voice broker.
    pub base_url: String,
    /// Service bearer token for broker calls.
    pub bearer_token: String,
    /// Request timeout for broker calls.
    pub request_timeout: std::time::Duration,
}

impl fmt::Debug for HttpVoiceBrokerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpVoiceBrokerConfig")
            .field("base_url", &self.base_url)
            .field("bearer_token", &"<redacted>")
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

/// Durable telemetry drain configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelemetryConfig {
    /// Selected sink backend.
    pub sink: TelemetrySinkConfig,
    /// Max events buffered before new events are dropped.
    pub queue_capacity: usize,
    /// Max events per write batch.
    pub batch_size: usize,
    /// Max time before a partial batch is flushed.
    pub flush_interval: std::time::Duration,
}

impl TelemetryConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let queue_capacity =
            optional_u32_env("SB_NETPLAY_TELEMETRY_QUEUE_CAPACITY", 20_000)? as usize;
        let batch_size = optional_u32_env("SB_NETPLAY_TELEMETRY_BATCH_SIZE", 250)? as usize;
        let flush_interval = optional_duration_millis_env("SB_NETPLAY_TELEMETRY_FLUSH_MS", 1000)?;

        let sink = match optional_env("SB_NETPLAY_POSTGRES_URL")? {
            Some(url) => {
                let dsn = PostgresDsn::parse(url).map_err(|_| ConfigError::InvalidPostgresDsn)?;

                TelemetrySinkConfig::Postgres(PostgresTelemetryConfig {
                    dsn,
                    tables: postgres_table_names_from_env()?,
                })
            }
            None => TelemetrySinkConfig::Disabled,
        };

        Ok(Self {
            batch_size: batch_size.max(1),
            flush_interval,
            queue_capacity: queue_capacity.max(1),
            sink,
        })
    }
}

/// Supported durable telemetry backends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TelemetrySinkConfig {
    /// Durable telemetry is disabled.
    Disabled,
    /// Drain sanitized room events to Postgres.
    Postgres(PostgresTelemetryConfig),
}

/// Postgres insert configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct PostgresTelemetryConfig {
    /// Postgres DSN.
    pub dsn: PostgresDsn,
    /// Table names receiving telemetry.
    pub tables: PostgresTableNames,
}

impl fmt::Debug for PostgresTelemetryConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresTelemetryConfig")
            .field("dsn", &self.dsn)
            .field("tables", &self.tables)
            .finish()
    }
}

/// Process logging output configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogConfig {
    /// Structured JSON or human compact logs.
    pub format: LogFormat,
}

/// Supported tracing output formats.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogFormat {
    /// Compact human-readable logs.
    Compact,
    /// JSON logs for production log collectors.
    Json,
}

fn required_env(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .map_err(|_| ConfigError::MissingEnv(name))
        .and_then(|value| {
            if value.trim().is_empty() {
                Err(ConfigError::EmptyEnv(name))
            } else {
                Ok(value)
            }
        })
}

fn optional_env(name: &'static str) -> Result<Option<String>, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value.trim().to_string())),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(None),
    }
}

fn postgres_table_names_from_env() -> Result<PostgresTableNames, ConfigError> {
    let legacy_events_table = optional_env("SB_NETPLAY_POSTGRES_TABLE")?;

    Ok(PostgresTableNames {
        events: optional_env("SB_NETPLAY_POSTGRES_EVENTS_TABLE")?
            .or(legacy_events_table)
            .unwrap_or_else(|| "netplay_room_events".to_string()),
        lobby_events: optional_env("SB_NETPLAY_POSTGRES_LOBBY_EVENTS_TABLE")?
            .unwrap_or_else(|| "netplay_lobby_events".to_string()),
        performance_samples: optional_env("SB_NETPLAY_POSTGRES_PERFORMANCE_TABLE")?
            .unwrap_or_else(|| "netplay_performance_samples".to_string()),
    })
}

fn optional_bool_env(name: &'static str, default: bool) -> Result<bool, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(ConfigError::InvalidBool(name)),
        },
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(default),
    }
}

fn optional_u32_env(name: &'static str, default: u32) -> Result<u32, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse()
            .map_err(|_| ConfigError::InvalidUnsigned(name)),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(default),
    }
}

fn optional_duration_seconds_env(
    name: &'static str,
    default_seconds: u64,
) -> Result<std::time::Duration, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u64>()
            .map(std::time::Duration::from_secs)
            .map_err(|_| ConfigError::InvalidUnsigned(name)),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(std::time::Duration::from_secs(default_seconds)),
    }
}

fn optional_duration_millis_env(
    name: &'static str,
    default_millis: u64,
) -> Result<std::time::Duration, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u64>()
            .map(std::time::Duration::from_millis)
            .map_err(|_| ConfigError::InvalidUnsigned(name)),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(std::time::Duration::from_millis(default_millis)),
    }
}

fn optional_log_format_env(
    name: &'static str,
    default: LogFormat,
) -> Result<LogFormat, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => match value.trim().to_ascii_lowercase().as_str() {
            "compact" => Ok(LogFormat::Compact),
            "json" => Ok(LogFormat::Json),
            _ => Err(ConfigError::InvalidLogFormat(name)),
        },
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(default),
    }
}

fn required_env_with_fallbacks(
    primary_name: &'static str,
    fallback_names: &[&'static str],
) -> Result<String, ConfigError> {
    match env::var(primary_name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) => Err(ConfigError::EmptyEnv(primary_name)),
        Err(_) => {
            for fallback_name in fallback_names {
                match optional_env(fallback_name)? {
                    Some(value) => return Ok(value),
                    None => continue,
                }
            }

            Err(ConfigError::MissingEnv(primary_name))
        }
    }
}

/// Configuration loading failure.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Required variable was absent.
    #[error("missing required environment variable {0}")]
    MissingEnv(&'static str),
    /// Required variable was present but blank.
    #[error("environment variable {0} cannot be blank")]
    EmptyEnv(&'static str),
    /// Bind address was not a valid socket address.
    #[error("invalid bind address")]
    InvalidBindAddr(#[from] AddrParseError),
    /// Boolean variable used an unsupported value.
    #[error("environment variable {0} must be true or false")]
    InvalidBool(&'static str),
    /// Unsigned integer variable used an unsupported value.
    #[error("environment variable {0} must be an unsigned integer")]
    InvalidUnsigned(&'static str),
    /// Log format variable used an unsupported value.
    #[error("environment variable {0} must be compact or json")]
    InvalidLogFormat(&'static str),
    /// Postgres telemetry DSN was not usable.
    #[error(
        "environment variable SB_NETPLAY_POSTGRES_URL must be postgres://user:pass@host:port/database with optional sslmode=require|prefer|disable|verify-ca|verify-full"
    )]
    InvalidPostgresDsn,
}
