//! Environment configuration for the netplay relay.
//!
//! This module owns parsing environment variables into typed configuration. It
//! does not construct services or start listeners.

use std::env;
use std::net::{AddrParseError, SocketAddr};

/// Runtime settings required to start the relay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    /// TCP address used by the HTTP/WebSocket server.
    pub bind_addr: SocketAddr,
    /// Full metadata-service URL used for desktop netplay authorization.
    pub desktop_authorize_url: String,
    /// Server-to-server secret sent to the license authority.
    pub license_internal_secret: String,
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

        Ok(Self {
            bind_addr,
            desktop_authorize_url,
            license_internal_secret,
        })
    }
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
}
