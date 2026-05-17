//! Shared service container for HTTP handlers.
//!
//! The container owns trait objects used by route handlers. It keeps dependency
//! construction outside individual routes.

use crate::auth::LicenseAuthority;
use crate::rooms::RoomRegistry;
use std::sync::Arc;

/// Dependencies required by HTTP routes.
#[derive(Clone)]
pub struct AppServices {
    /// License verifier used before creating or joining rooms.
    pub license_authority: Arc<dyn LicenseAuthority>,
    /// Active room registry.
    pub rooms: Arc<dyn RoomRegistry>,
}

impl AppServices {
    /// Creates a service container from independently testable dependencies.
    pub fn new(license_authority: Arc<dyn LicenseAuthority>, rooms: Arc<dyn RoomRegistry>) -> Self {
        Self {
            license_authority,
            rooms,
        }
    }
}
