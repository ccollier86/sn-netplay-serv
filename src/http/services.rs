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
    /// Whether large save-state transfer tickets may be created.
    pub save_states_enabled: bool,
}

impl FileRelayPolicy {
    /// Returns whether temporary ROM relay can be used for this request.
    pub fn can_relay_temporary_roms(&self, broker: &dyn FileRelayBroker) -> bool {
        self.temporary_roms_enabled && broker.is_enabled()
    }

    /// Returns whether large save-state relay can be used for this request.
    pub fn can_relay_save_states(&self, broker: &dyn FileRelayBroker) -> bool {
        self.save_states_enabled && broker.is_enabled()
    }
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

#[cfg(test)]
mod tests {
    use super::FileRelayPolicy;
    use crate::file_relay::{
        CreateFileRelayTransferRequest, CreateFileRelayTransferResponse, DisabledFileRelayBroker,
        FileRelayBroker, FileRelayBrokerError,
    };

    #[test]
    fn file_relay_policy_enforces_independent_feature_switches() {
        let enabled_broker = EnabledFileRelayBroker;
        let disabled_broker = DisabledFileRelayBroker;
        let policy = FileRelayPolicy {
            save_states_enabled: false,
            temporary_rom_max_bytes: 1024,
            temporary_roms_enabled: true,
        };

        assert!(policy.can_relay_temporary_roms(&enabled_broker));
        assert!(!policy.can_relay_temporary_roms(&disabled_broker));
        assert!(!policy.can_relay_save_states(&enabled_broker));
    }

    struct EnabledFileRelayBroker;

    #[async_trait::async_trait]
    impl FileRelayBroker for EnabledFileRelayBroker {
        fn is_enabled(&self) -> bool {
            true
        }

        fn public_base_url(&self) -> Option<&str> {
            Some("https://relay.shadowboy.app")
        }

        async fn create_transfer(
            &self,
            _request: CreateFileRelayTransferRequest,
        ) -> Result<CreateFileRelayTransferResponse, FileRelayBrokerError> {
            Err(FileRelayBrokerError::RequestFailed)
        }
    }
}
