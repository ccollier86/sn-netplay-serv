//! Shared service container for HTTP handlers.
//!
//! The container owns trait objects used by route handlers. It keeps dependency
//! construction outside individual routes.

use crate::auth::LicenseAuthority;
use crate::file_relay::FileRelayBroker;
use crate::http::AdminAuthorizer;
use crate::lobbies::LobbyRegistry;
use crate::observability::MetricsRecorder;
use crate::rate_limit::RateLimiter;
use crate::rooms::RoomRegistry;
use std::sync::Arc;

/// Runtime file-relay policy used by transport handlers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileRelayPolicy {
    /// Whether temporary ROM transfer tickets may be created.
    pub temporary_roms_enabled: bool,
    /// Maximum temporary ROM payload bytes accepted by netplay.
    pub temporary_rom_max_bytes: u64,
}

/// Dependencies required by HTTP routes.
#[derive(Clone)]
pub struct AppServices {
    /// License verifier used before creating or joining rooms.
    pub license_authority: Arc<dyn LicenseAuthority>,
    /// Active room registry.
    pub rooms: Arc<dyn RoomRegistry>,
    /// Persistent multiplayer lobby registry.
    pub lobbies: Arc<dyn LobbyRegistry>,
    /// Trusted temporary file relay broker.
    pub file_relay: Arc<dyn FileRelayBroker>,
    /// File-relay feature and size policy.
    pub file_relay_policy: FileRelayPolicy,
    /// Public request limiter.
    pub rate_limiter: Arc<dyn RateLimiter>,
    /// Process metrics recorder.
    pub metrics: Arc<dyn MetricsRecorder>,
    /// Internal endpoint authorizer.
    pub admin_authorizer: AdminAuthorizer,
    /// Whether proxy forwarding headers are trusted for client identity.
    pub trust_proxy_headers: bool,
}

impl AppServices {
    /// Creates a service container from independently testable dependencies.
    pub fn new(
        license_authority: Arc<dyn LicenseAuthority>,
        rooms: Arc<dyn RoomRegistry>,
        lobbies: Arc<dyn LobbyRegistry>,
        file_relay: Arc<dyn FileRelayBroker>,
        file_relay_policy: FileRelayPolicy,
        rate_limiter: Arc<dyn RateLimiter>,
        metrics: Arc<dyn MetricsRecorder>,
        admin_authorizer: AdminAuthorizer,
        trust_proxy_headers: bool,
    ) -> Self {
        Self {
            license_authority,
            rooms,
            lobbies,
            file_relay,
            file_relay_policy,
            rate_limiter,
            metrics,
            admin_authorizer,
            trust_proxy_headers,
        }
    }
}
