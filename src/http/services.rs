//! Shared service container for HTTP handlers.
//!
//! The container owns trait objects used by route handlers. It keeps dependency
//! construction outside individual routes.

use crate::auth::LicenseAuthority;
use crate::http::AdminAuthorizer;
use crate::observability::MetricsRecorder;
use crate::rate_limit::RateLimiter;
use crate::rooms::RoomRegistry;
use std::sync::Arc;

/// Dependencies required by HTTP routes.
#[derive(Clone)]
pub struct AppServices {
    /// License verifier used before creating or joining rooms.
    pub license_authority: Arc<dyn LicenseAuthority>,
    /// Active room registry.
    pub rooms: Arc<dyn RoomRegistry>,
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
        rate_limiter: Arc<dyn RateLimiter>,
        metrics: Arc<dyn MetricsRecorder>,
        admin_authorizer: AdminAuthorizer,
        trust_proxy_headers: bool,
    ) -> Self {
        Self {
            license_authority,
            rooms,
            rate_limiter,
            metrics,
            admin_authorizer,
            trust_proxy_headers,
        }
    }
}
