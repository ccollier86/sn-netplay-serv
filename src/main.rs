//! Binary entry point for the ShadowBoy netplay relay.
//!
//! This module only wires configuration, logging, routing, and process
//! lifecycle. Domain rules live in smaller modules under `auth`, `rooms`, and
//! `protocol`.

use sb_netplay_serv::auth::HttpLicenseAuthority;
use sb_netplay_serv::config::{ServerConfig, VoiceBrokerConfig};
use sb_netplay_serv::file_relay::{
    DisabledFileRelayBroker, FileRelayBroker, FileRelayBrokerConfig, HttpFileRelayBroker,
};
use sb_netplay_serv::http::{
    AdminAuthorizer, AppServiceDependencies, AppServices, FileRelayPolicy, build_router,
};
use sb_netplay_serv::lobbies::{
    InMemoryLobbyRegistry, LobbyServerCapabilities, MAX_LOBBY_PLAYERS, spawn_lobby_expiration_task,
};
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
    let (event_sink, lobby_event_sink, _telemetry_task) =
        spawn_telemetry_sink(config.telemetry.clone(), metrics.clone());
    let voice_broker: Arc<dyn VoiceBroker> = match config.voice.broker.clone() {
        VoiceBrokerConfig::Disabled => Arc::new(DisabledVoiceBroker),
        VoiceBrokerConfig::Http(voice) => Arc::new(HttpVoiceBroker::new(
            voice.base_url,
            voice.bearer_token,
            voice.request_timeout,
        )?),
    };
    let file_relay_broker: Arc<dyn FileRelayBroker> = match config.file_relay.broker.clone() {
        FileRelayBrokerConfig::Disabled => Arc::new(DisabledFileRelayBroker),
        FileRelayBrokerConfig::Http(file_relay) => Arc::new(HttpFileRelayBroker::new(
            file_relay.base_url,
            file_relay.bearer_token,
            file_relay.request_timeout,
        )?),
    };
    match &config.file_relay.broker {
        FileRelayBrokerConfig::Disabled => {
            info!(
                temporary_roms_enabled = config.file_relay.temporary_roms_enabled,
                direct_roms_enabled = config.file_relay.direct_roms_enabled,
                save_states_enabled = config.file_relay.save_states_enabled,
                temporary_rom_max_bytes = config.file_relay.temporary_rom_max_bytes,
                direct_rom_allowed_systems = ?config.file_relay.direct_rom_allowed_systems,
                "file relay broker disabled"
            );
        }
        FileRelayBrokerConfig::Http(file_relay) => {
            info!(
                base_url = %file_relay.base_url,
                timeout_ms = file_relay.request_timeout.as_millis(),
                temporary_roms_enabled = config.file_relay.temporary_roms_enabled,
                direct_roms_enabled = config.file_relay.direct_roms_enabled,
                save_states_enabled = config.file_relay.save_states_enabled,
                temporary_rom_max_bytes = config.file_relay.temporary_rom_max_bytes,
                direct_rom_allowed_systems = ?config.file_relay.direct_rom_allowed_systems,
                "file relay broker configured"
            );
        }
    }
    let lobby_capabilities = LobbyServerCapabilities::current(
        MAX_LOBBY_PLAYERS,
        file_relay_broker.is_enabled() && config.file_relay.temporary_roms_enabled,
        voice_broker.is_enabled(),
    );
    let rooms = Arc::new(InMemoryRoomRegistry::with_dependencies_and_event_sink(
        Arc::new(UuidInviteCodeGenerator),
        Arc::new(sb_netplay_serv::rooms::UuidResumeTokenGenerator),
        Arc::new(sb_netplay_serv::rooms::SystemClock),
        config.recovery,
        event_sink,
    ));
    let lobbies = Arc::new(
        InMemoryLobbyRegistry::with_generators_capabilities_voice_and_event_sink(
            Arc::new(UuidInviteCodeGenerator),
            Arc::new(sb_netplay_serv::rooms::UuidResumeTokenGenerator),
            lobby_capabilities,
            voice_broker,
            lobby_event_sink,
        ),
    );
    let rate_limiter = Arc::new(InMemoryRateLimiter::new(config.rate_limits));
    let admin_authorizer = AdminAuthorizer::new(config.admin_token.clone());
    let _room_expiration_task = spawn_room_expiration_task(rooms.clone());
    let _room_frame_clock_task = spawn_room_frame_clock_task(rooms.clone());
    let _lobby_expiration_task = spawn_lobby_expiration_task(lobbies.clone(), config.lobby_idle);
    let services = AppServices::new(AppServiceDependencies {
        license_authority,
        rooms,
        lobbies,
        file_relay: file_relay_broker,
        file_relay_policy: FileRelayPolicy {
            save_states_enabled: config.file_relay.save_states_enabled,
            temporary_roms_enabled: config.file_relay.temporary_roms_enabled,
            direct_roms_enabled: config.file_relay.direct_roms_enabled,
            temporary_rom_max_bytes: config.file_relay.temporary_rom_max_bytes,
            direct_rom_allowed_systems: config.file_relay.direct_rom_allowed_systems.clone(),
        },
        rate_limiter,
        metrics,
        protocol_rollout: config.protocol_rollout,
        admin_authorizer,
        trust_proxy_headers: config.trust_proxy_headers,
    });
    let app = build_router(services);
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;

    info!(
        bind_addr = %config.bind_addr,
        lobby_idle_seconds = config.lobby_idle.as_secs(),
        "starting ShadowBoy netplay server"
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
