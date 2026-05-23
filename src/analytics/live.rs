//! Live relay diagnostics for operator CLI commands.

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::Value;
use std::env;

const DEFAULT_RELAY_BASE_URL: &str = "https://netplay.shadowboy.app";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveDiagnosticsConfig {
    pub admin_token: String,
    pub relay_base_url: String,
}

impl LiveDiagnosticsConfig {
    /// Builds live diagnostics config from operator environment.
    pub fn from_env() -> Result<Self, LiveDiagnosticsConfigError> {
        let admin_token = required_env("SB_NETPLAY_ADMIN_TOKEN")
            .ok_or(LiveDiagnosticsConfigError::MissingToken)?;
        let relay_base_url = optional_env("SB_NETPLAY_LIVE_URL")
            .or_else(|| optional_env("SERVICE_URL_NETPLAY"))
            .unwrap_or_else(|| DEFAULT_RELAY_BASE_URL.to_string());

        Ok(Self {
            admin_token,
            relay_base_url: relay_base_url.trim_end_matches('/').to_string(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LiveDiagnosticsConfigError {
    #[error("SB_NETPLAY_ADMIN_TOKEN is required for live diagnostics")]
    MissingToken,
}

pub struct LiveDiagnosticsClient {
    client: reqwest::Client,
    config: LiveDiagnosticsConfig,
}

impl LiveDiagnosticsClient {
    /// Creates a live diagnostics HTTP client.
    pub fn new(config: LiveDiagnosticsConfig) -> Result<Self, LiveDiagnosticsError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", config.admin_token))
                .map_err(|_| LiveDiagnosticsError::InvalidToken)?,
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self { client, config })
    }

    /// Fetches process metrics from the live relay.
    pub async fn metrics(&self) -> Result<Value, LiveDiagnosticsError> {
        self.get_json("/internal/metrics").await
    }

    /// Fetches active rooms from the live relay.
    pub async fn rooms(&self) -> Result<Value, LiveDiagnosticsError> {
        self.get_json("/internal/rooms").await
    }

    /// Fetches one active room by invite code.
    pub async fn room(&self, invite_code: &str) -> Result<Value, LiveDiagnosticsError> {
        self.get_json(&format!("/internal/rooms/{}", encode_path(invite_code)))
            .await
    }

    /// Fetches recent events across live rooms.
    pub async fn recent_events(&self, limit: usize) -> Result<Value, LiveDiagnosticsError> {
        self.get_json(&format!(
            "/internal/recent-events?limit={}",
            bounded_limit(limit)
        ))
        .await
    }

    /// Fetches recent events for one live room by invite code.
    pub async fn room_events(
        &self,
        invite_code: &str,
        limit: usize,
    ) -> Result<Value, LiveDiagnosticsError> {
        self.get_json(&format!(
            "/internal/rooms/{}/events?limit={}",
            encode_path(invite_code),
            bounded_limit(limit)
        ))
        .await
    }

    async fn get_json(&self, path_and_query: &str) -> Result<Value, LiveDiagnosticsError> {
        let url = format!("{}{}", self.config.relay_base_url, path_and_query);
        let response = self.client.get(url).send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LiveDiagnosticsError::HttpStatus {
                body,
                status: status.as_u16(),
            });
        }

        Ok(response.json().await?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LiveDiagnosticsError {
    #[error("SB_NETPLAY_ADMIN_TOKEN cannot be used as an HTTP header value")]
    InvalidToken,
    #[error("live diagnostics request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("live diagnostics endpoint returned HTTP {status}: {body}")]
    HttpStatus { status: u16, body: String },
}

fn bounded_limit(limit: usize) -> usize {
    limit.clamp(1, 500)
}

fn encode_path(value: &str) -> String {
    value.trim().replace(' ', "").to_ascii_uppercase()
}

fn optional_env(name: &'static str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn required_env(name: &'static str) -> Option<String> {
    optional_env(name)
}
