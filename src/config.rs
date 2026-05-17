//! Environment configuration for the netplay relay.
//!
//! This module owns parsing environment variables into typed configuration. It
//! does not construct services or start listeners.

use std::env;
use std::net::{AddrParseError, SocketAddr};

use crate::rate_limit::RateLimitPolicy;

/// Runtime settings required to start the relay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    /// TCP address used by the HTTP/WebSocket server.
    pub bind_addr: SocketAddr,
    /// Full metadata-service URL used for desktop netplay authorization.
    pub desktop_authorize_url: String,
    /// Server-to-server secret sent to the license authority.
    pub license_internal_secret: String,
    /// Optional bearer token required by internal observability endpoints.
    pub admin_token: Option<String>,
    /// Whether request identity may use proxy forwarding headers.
    pub trust_proxy_headers: bool,
    /// Per-action request rate limits.
    pub rate_limits: RateLimitPolicy,
    /// Logging output settings.
    pub log: LogConfig,
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
        let desktop_authorize_url = required_env_with_fallback(
            "SB_NETPLAY_DESKTOP_AUTHORIZE_URL",
            "SB_NETPLAY_LICENSE_VERIFY_URL",
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
        let log = LogConfig {
            format: optional_log_format_env("SB_NETPLAY_LOG_FORMAT", LogFormat::Compact)?,
        };

        Ok(Self {
            bind_addr,
            desktop_authorize_url,
            license_internal_secret,
            admin_token,
            trust_proxy_headers,
            rate_limits,
            log,
        })
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

fn required_env_with_fallback(
    primary_name: &'static str,
    fallback_name: &'static str,
) -> Result<String, ConfigError> {
    match env::var(primary_name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) => Err(ConfigError::EmptyEnv(primary_name)),
        Err(_) => required_env(fallback_name),
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
}
