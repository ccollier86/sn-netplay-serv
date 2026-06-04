//! File relay configuration.
//!
//! This module keeps file-relay env parsing out of the main server config file.

use crate::config::ConfigError;
use std::env;
use std::fmt;
use std::time::Duration;

/// Optional file relay integration configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRelayConfig {
    /// Selected file relay broker backend.
    pub broker: FileRelayBrokerConfig,
    /// Whether temporary ROM relay tickets may be created.
    pub temporary_roms_enabled: bool,
    /// Whether Android direct-invite temporary ROM relay tickets may be created.
    pub direct_roms_enabled: bool,
    /// Maximum temporary ROM bytes accepted for one lobby transfer.
    pub temporary_rom_max_bytes: u64,
    /// Systems allowed to use direct-invite ROM relay.
    pub direct_rom_allowed_systems: Vec<String>,
    /// Whether large save-state relay tickets may be created.
    pub save_states_enabled: bool,
}

impl FileRelayConfig {
    /// Reads file relay configuration from process environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        let broker_url = optional_env("SB_NETPLAY_FILE_RELAY_URL")?;
        let broker_token = optional_env("SB_NETPLAY_FILE_RELAY_TOKEN")?;
        let request_timeout =
            optional_duration_millis_env("SB_NETPLAY_FILE_RELAY_TIMEOUT_MS", 2500)?;
        let temporary_roms_enabled = optional_bool_env("SB_NETPLAY_ROM_RELAY_ENABLED", false)?;
        let direct_roms_enabled = optional_bool_env("SB_NETPLAY_DIRECT_ROM_RELAY_ENABLED", false)?;
        let temporary_rom_max_bytes =
            optional_u64_env("SB_NETPLAY_ROM_RELAY_MAX_BYTES", 104_857_600)?;
        let direct_rom_allowed_systems = optional_csv_env(
            "SB_NETPLAY_DIRECT_ROM_RELAY_ALLOWED_SYSTEMS",
            &[
                "gb",
                "gbc",
                "gameboy",
                "gameboy-color",
                "gba",
                "nes",
                "snes",
                "genesis",
                "sms",
                "master-system",
                "game-gear",
            ],
        )?;
        let save_states_enabled =
            optional_bool_env("SB_NETPLAY_FILE_RELAY_SAVE_STATES_ENABLED", true)?;

        let broker = match (broker_url, broker_token) {
            (None, None) => FileRelayBrokerConfig::Disabled,
            (Some(base_url), Some(bearer_token)) => {
                FileRelayBrokerConfig::Http(HttpFileRelayBrokerConfig {
                    base_url,
                    bearer_token,
                    request_timeout,
                })
            }
            (Some(_), None) => {
                return Err(ConfigError::MissingEnv("SB_NETPLAY_FILE_RELAY_TOKEN"));
            }
            (None, Some(_)) => return Err(ConfigError::MissingEnv("SB_NETPLAY_FILE_RELAY_URL")),
        };

        Ok(Self {
            broker,
            temporary_rom_max_bytes,
            temporary_roms_enabled,
            direct_roms_enabled,
            direct_rom_allowed_systems,
            save_states_enabled,
        })
    }

    /// Returns whether a configured broker can issue transfer tickets.
    pub fn broker_enabled(&self) -> bool {
        matches!(self.broker, FileRelayBrokerConfig::Http(_))
    }
}

/// Supported file relay broker backends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileRelayBrokerConfig {
    /// File relay integration is disabled.
    Disabled,
    /// HTTP broker compatible with `sb-file-relay-serv`.
    Http(HttpFileRelayBrokerConfig),
}

/// HTTP file relay broker configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct HttpFileRelayBrokerConfig {
    /// Base URL for the trusted file relay.
    pub base_url: String,
    /// Service bearer token for relay calls.
    pub bearer_token: String,
    /// Request timeout for relay calls.
    pub request_timeout: Duration,
}

impl fmt::Debug for HttpFileRelayBrokerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpFileRelayBrokerConfig")
            .field("base_url", &self.base_url)
            .field("bearer_token", &"<redacted>")
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
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

fn optional_duration_millis_env(
    name: &'static str,
    default_millis: u64,
) -> Result<Duration, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| ConfigError::InvalidUnsigned(name)),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(Duration::from_millis(default_millis)),
    }
}

fn optional_u64_env(name: &'static str, default: u64) -> Result<u64, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidUnsigned(name)),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(default),
    }
}

fn optional_csv_env(name: &'static str, default: &[&str]) -> Result<Vec<String>, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => {
            let values = value
                .split(',')
                .map(|part| part.trim().to_ascii_lowercase())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            if values.is_empty() {
                return Err(ConfigError::EmptyEnv(name));
            }
            Ok(values)
        }
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(default.iter().map(|value| value.to_string()).collect()),
    }
}
