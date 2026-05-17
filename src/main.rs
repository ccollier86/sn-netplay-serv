//! Binary entry point for the ShadowBoy netplay relay.
//!
//! This module only wires configuration, logging, routing, and process
//! lifecycle. Domain rules live in smaller modules under `auth`, `rooms`, and
//! `protocol`.

use sb_netplay_serv::auth::HttpLicenseAuthority;
use sb_netplay_serv::config::ServerConfig;
use sb_netplay_serv::http::{AdminAuthorizer, AppServices, build_router};
use sb_netplay_serv::observability::{InMemoryMetrics, init_tracing};
use sb_netplay_serv::rate_limit::InMemoryRateLimiter;
use sb_netplay_serv::rooms::{
    InMemoryRoomRegistry, UuidInviteCodeGenerator, spawn_room_expiration_task,
};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env()?;
    init_tracing(config.log);

    let license_authority = Arc::new(HttpLicenseAuthority::new(
        config.desktop_authorize_url.clone(),
        config.license_internal_secret.clone(),
    )?);
    let rooms = Arc::new(InMemoryRoomRegistry::new(Arc::new(UuidInviteCodeGenerator)));
    let rate_limiter = Arc::new(InMemoryRateLimiter::new(config.rate_limits));
    let metrics = Arc::new(InMemoryMetrics::new());
    let admin_authorizer = AdminAuthorizer::new(config.admin_token.clone());
    let _room_expiration_task = spawn_room_expiration_task(rooms.clone());
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
