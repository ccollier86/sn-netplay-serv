//! Binary entry point for the ShadowBoy netplay relay.
//!
//! This module only wires configuration, logging, routing, and process
//! lifecycle. Domain rules live in smaller modules under `auth`, `rooms`, and
//! `protocol`.

use sb_netplay_serv::auth::HttpLicenseAuthority;
use sb_netplay_serv::config::{ServerConfig, VoiceBrokerConfig};
use sb_netplay_serv::http::{AdminAuthorizer, AppServices, build_router};
use sb_netplay_serv::observability::{
    InMemoryMetrics, ensure_telemetry_schema, init_tracing, spawn_telemetry_sink,
};
use sb_netplay_serv::rate_limit::InMemoryRateLimiter;
use sb_netplay_serv::rooms::{
    InMemoryRoomRegistry, UuidInviteCodeGenerator, spawn_room_expiration_task,
    spawn_room_frame_clock_task,
};
use sb_netplay_serv::voice::{DisabledVoiceBroker, HttpVoiceBroker, VoiceBroker};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env()?;
    init_tracing(config.log);

    let license_authority = Arc::new(HttpLicenseAuthority::new(
        config.authorize_url.clone(),
        config.license_internal_secret.clone(),
    )?);
    ensure_telemetry_schema(&config.telemetry).await?;
    let metrics = Arc::new(InMemoryMetrics::new());
    let (event_sink, _telemetry_task) =
        spawn_telemetry_sink(config.telemetry.clone(), metrics.clone());
    let voice_broker: Arc<dyn VoiceBroker> = match config.voice.broker.clone() {
        VoiceBrokerConfig::Disabled => Arc::new(DisabledVoiceBroker),
        VoiceBrokerConfig::Http(voice) => Arc::new(HttpVoiceBroker::new(
            voice.base_url,
            voice.bearer_token,
            voice.request_timeout,
        )?),
    };
    let rooms = Arc::new(
        InMemoryRoomRegistry::with_dependencies_event_sink_and_voice(
            Arc::new(UuidInviteCodeGenerator),
            Arc::new(sb_netplay_serv::rooms::UuidResumeTokenGenerator),
            Arc::new(sb_netplay_serv::rooms::SystemClock),
            config.recovery,
            event_sink,
            voice_broker,
        ),
    );
    let rate_limiter = Arc::new(InMemoryRateLimiter::new(config.rate_limits));
    let admin_authorizer = AdminAuthorizer::new(config.admin_token.clone());
    let _room_expiration_task = spawn_room_expiration_task(rooms.clone());
    let _room_frame_clock_task = spawn_room_frame_clock_task(rooms.clone());
    let services = AppServices::new(
        license_authority,
        rooms,
        rate_limiter,
        metrics,
        admin_authorizer,
        config.trust_proxy_headers,
    );
    let app = build_router(services);
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;

    info!(bind_addr = %config.bind_addr, "starting ShadowBoy netplay server");
    axum::serve(listener, app).await?;

    Ok(())
}
