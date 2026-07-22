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
        Self::from_lookup(env::var)
    }

    fn from_lookup(
        mut lookup: impl FnMut(&'static str) -> Result<String, env::VarError>,
    ) -> Result<Self, ConfigError> {
        let broker_url = optional_env(&mut lookup, "SB_NETPLAY_FILE_RELAY_URL")?;
        let broker_token = optional_env(&mut lookup, "SB_NETPLAY_FILE_RELAY_TOKEN")?;
        let request_timeout =
            optional_duration_millis_env(&mut lookup, "SB_NETPLAY_FILE_RELAY_TIMEOUT_MS", 2500)?;
        let temporary_roms_enabled =
            optional_bool_env(&mut lookup, "SB_NETPLAY_ROM_RELAY_ENABLED", false)?;
        let direct_roms_enabled =
            optional_bool_env(&mut lookup, "SB_NETPLAY_DIRECT_ROM_RELAY_ENABLED", false)?;
        let temporary_rom_max_bytes =
            optional_u64_env(&mut lookup, "SB_NETPLAY_ROM_RELAY_MAX_BYTES", 104_857_600)?;
        let direct_rom_allowed_systems = optional_csv_env(
            &mut lookup,
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
        let save_states_enabled = optional_bool_env(
            &mut lookup,
            "SB_NETPLAY_FILE_RELAY_SAVE_STATES_ENABLED",
            true,
        )?;

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

fn optional_env(
    lookup: &mut impl FnMut(&'static str) -> Result<String, env::VarError>,
    name: &'static str,
) -> Result<Option<String>, ConfigError> {
    match lookup(name) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value.trim().to_string())),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(None),
    }
}

fn optional_bool_env(
    lookup: &mut impl FnMut(&'static str) -> Result<String, env::VarError>,
    name: &'static str,
    default: bool,
) -> Result<bool, ConfigError> {
    match lookup(name) {
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
    lookup: &mut impl FnMut(&'static str) -> Result<String, env::VarError>,
    name: &'static str,
    default_millis: u64,
) -> Result<Duration, ConfigError> {
    match lookup(name) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| ConfigError::InvalidUnsigned(name)),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(Duration::from_millis(default_millis)),
    }
}

fn optional_u64_env(
    lookup: &mut impl FnMut(&'static str) -> Result<String, env::VarError>,
    name: &'static str,
    default: u64,
) -> Result<u64, ConfigError> {
    match lookup(name) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u64>()
            .map_err(|_| ConfigError::InvalidUnsigned(name)),
        Ok(_) => Err(ConfigError::EmptyEnv(name)),
        Err(_) => Ok(default),
    }
}

fn optional_csv_env(
    lookup: &mut impl FnMut(&'static str) -> Result<String, env::VarError>,
    name: &'static str,
    default: &[&str],
) -> Result<Vec<String>, ConfigError> {
    match lookup(name) {
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

#[cfg(test)]
mod tests {
    use super::{FileRelayBrokerConfig, FileRelayConfig};
    use crate::config::ConfigError;
    use std::collections::BTreeMap;
    use std::env;
    use std::time::Duration;

    const DEFAULT_ALLOWED_SYSTEMS: [&str; 11] = [
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
    ];

    #[test]
    fn omitted_file_relay_environment_disables_broker_and_uses_defaults() {
        let config = parse(&[]).expect("omitted optional file-relay environment");

        assert_eq!(config.broker, FileRelayBrokerConfig::Disabled);
        assert_defaults(&config);
    }

    #[test]
    fn url_and_token_are_the_minimum_enabled_environment() {
        let config = parse(&[
            ("SB_NETPLAY_FILE_RELAY_URL", "https://relay.shadowboy.test"),
            ("SB_NETPLAY_FILE_RELAY_TOKEN", "test-service-token"),
        ])
        .expect("minimum enabled file-relay environment");

        let FileRelayBrokerConfig::Http(broker) = &config.broker else {
            panic!("minimum enabled environment must select the HTTP broker");
        };
        assert_eq!(broker.base_url, "https://relay.shadowboy.test");
        assert_eq!(broker.bearer_token, "test-service-token");
        assert_eq!(broker.request_timeout, Duration::from_millis(2_500));
        assert_defaults(&config);
    }

    #[test]
    fn one_sided_broker_environment_is_rejected() {
        assert!(matches!(
            parse(&[("SB_NETPLAY_FILE_RELAY_URL", "https://relay.shadowboy.test")]),
            Err(ConfigError::MissingEnv("SB_NETPLAY_FILE_RELAY_TOKEN"))
        ));
    }

    #[test]
    fn explicit_blank_optional_value_is_rejected() {
        assert!(matches!(
            parse(&[("SB_NETPLAY_ROM_RELAY_ENABLED", "")]),
            Err(ConfigError::EmptyEnv("SB_NETPLAY_ROM_RELAY_ENABLED"))
        ));
    }

    fn parse(values: &[(&'static str, &str)]) -> Result<FileRelayConfig, ConfigError> {
        let values = values.iter().copied().collect::<BTreeMap<_, _>>();
        FileRelayConfig::from_lookup(|name| {
            values
                .get(name)
                .map(|value| (*value).to_string())
                .ok_or(env::VarError::NotPresent)
        })
    }

    fn assert_defaults(config: &FileRelayConfig) {
        assert!(!config.temporary_roms_enabled);
        assert!(!config.direct_roms_enabled);
        assert_eq!(config.temporary_rom_max_bytes, 104_857_600);
        assert_eq!(
            config.direct_rom_allowed_systems,
            DEFAULT_ALLOWED_SYSTEMS.map(str::to_string).to_vec()
        );
        assert!(config.save_states_enabled);
    }
}
