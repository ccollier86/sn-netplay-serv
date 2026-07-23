//! Shared service container for HTTP handlers.
//!
//! The container owns trait objects used by route handlers. It keeps dependency
//! construction outside individual routes.

use crate::auth::LicenseAuthority;
use crate::file_relay::FileRelayBroker;
use crate::http::AdminAuthorizer;
use crate::http::errors::HttpError;
use crate::lobbies::LobbyRegistry;
use crate::observability::MetricsRecorder;
use crate::protocol::NetplayProtocolRolloutPolicy;
use crate::protocol::{
    LINK_CABLE_CONTRACT_VERSION, NetplayClientKind, NetplayRoomMode, NetplaySessionDescriptor,
    NetplaySessionMode, RomRelayCapability, RomRelayCapabilityReason, RomRelayIntent,
};
use crate::rate_limit::RateLimiter;
use crate::rooms::RoomRegistry;
use std::sync::Arc;

/// Runtime file-relay policy used by transport handlers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRelayPolicy {
    /// Whether temporary ROM transfer tickets may be created.
    pub temporary_roms_enabled: bool,
    /// Whether Android direct-invite ROM transfer tickets may be created.
    pub direct_roms_enabled: bool,
    /// Maximum temporary ROM payload bytes accepted by netplay.
    pub temporary_rom_max_bytes: u64,
    /// Direct-invite systems allowed by policy.
    pub direct_rom_allowed_systems: Vec<String>,
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

    /// Returns whether direct Android ROM relay can be used for this request.
    pub fn can_relay_direct_roms(&self, broker: &dyn FileRelayBroker) -> bool {
        self.direct_roms_enabled && broker.is_enabled()
    }

    /// Computes the capability view returned to direct-invite clients.
    pub fn direct_rom_relay_capability(
        &self,
        broker: &dyn FileRelayBroker,
        session: &NetplaySessionDescriptor,
    ) -> Option<RomRelayCapability> {
        if session.room_mode != NetplayRoomMode::DirectInvite
            || session.host_client_kind != Some(NetplayClientKind::Android)
            || session.rom_relay_intent != RomRelayIntent::MissingPeerOnly
        {
            return None;
        }

        let mut reason = None;
        if !self.direct_roms_enabled {
            reason = Some(RomRelayCapabilityReason::Disabled);
        } else if !broker.is_enabled() {
            reason = Some(RomRelayCapabilityReason::BrokerUnavailable);
        } else if let Some(identity) = session.rom_identity.as_ref() {
            if identity.size_bytes > self.temporary_rom_max_bytes {
                reason = Some(RomRelayCapabilityReason::TooLarge);
            } else if !self
                .direct_rom_allowed_systems
                .iter()
                .any(|system| system == &identity.system)
            {
                reason = Some(RomRelayCapabilityReason::UnsupportedSystem);
            }
        } else {
            reason = Some(RomRelayCapabilityReason::MissingIdentity);
        }

        Some(RomRelayCapability {
            supported: true,
            available: reason.is_none(),
            temporary_access_only: true,
            max_bytes: self.temporary_rom_max_bytes,
            allowed_systems: self.direct_rom_allowed_systems.clone(),
            reason,
        })
    }
}

/// Default-off availability policy for the mGBA link-cable session provider.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LinkCableRolloutPolicy {
    enabled: bool,
}

impl LinkCableRolloutPolicy {
    /// Creates a provider rollout policy from server configuration.
    pub const fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Returns whether the provider is available for new sessions.
    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    /// Rejects link sessions unless both the server and client completed this contract.
    pub(crate) fn validate_provider_admission(
        self,
        session: &NetplaySessionDescriptor,
        client_contract_version: Option<u16>,
    ) -> Result<(), HttpError> {
        if session.mode != NetplaySessionMode::LinkCable {
            return Ok(());
        }

        if !self.enabled {
            return Err(HttpError::InvalidRequest {
                code: "linkCableUnavailable",
                message: "Link-cable multiplayer is not available on this server.",
            });
        }

        match client_contract_version {
            None => Err(HttpError::InvalidRequest {
                code: "linkCableCapabilityRequired",
                message: "This link room requires an explicit linkContractVersion.",
            }),
            Some(LINK_CABLE_CONTRACT_VERSION) => Ok(()),
            Some(_) => Err(HttpError::InvalidRequest {
                code: "linkCableCapabilityUnsupported",
                message: "This client does not support the server's link-cable contract.",
            }),
        }
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
    /// Platform-scoped netplay protocol rollout policy.
    pub protocol_rollout: NetplayProtocolRolloutPolicy,
    /// Default-off mGBA link-cable provider rollout policy.
    pub link_cable_rollout: LinkCableRolloutPolicy,
    /// Internal endpoint authorizer.
    pub admin_authorizer: AdminAuthorizer,
    /// Whether proxy forwarding headers are trusted for client identity.
    pub trust_proxy_headers: bool,
}

/// Dependency bundle used to assemble route services.
pub struct AppServiceDependencies {
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
    /// Platform-scoped netplay protocol rollout policy.
    pub protocol_rollout: NetplayProtocolRolloutPolicy,
    /// Default-off mGBA link-cable provider rollout policy.
    pub link_cable_rollout: LinkCableRolloutPolicy,
    /// Internal endpoint authorizer.
    pub admin_authorizer: AdminAuthorizer,
    /// Whether proxy forwarding headers are trusted for client identity.
    pub trust_proxy_headers: bool,
}

impl AppServices {
    /// Creates a service container from independently testable dependencies.
    pub fn new(dependencies: AppServiceDependencies) -> Self {
        Self {
            license_authority: dependencies.license_authority,
            rooms: dependencies.rooms,
            lobbies: dependencies.lobbies,
            file_relay: dependencies.file_relay,
            file_relay_policy: dependencies.file_relay_policy,
            rate_limiter: dependencies.rate_limiter,
            metrics: dependencies.metrics,
            protocol_rollout: dependencies.protocol_rollout,
            link_cable_rollout: dependencies.link_cable_rollout,
            admin_authorizer: dependencies.admin_authorizer,
            trust_proxy_headers: dependencies.trust_proxy_headers,
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
    use crate::protocol::{NetplaySessionDescriptor, RomRelayCapabilityReason};
    use serde_json::json;

    #[test]
    fn file_relay_policy_enforces_independent_feature_switches() {
        let enabled_broker = EnabledFileRelayBroker;
        let disabled_broker = DisabledFileRelayBroker;
        let policy = FileRelayPolicy {
            direct_roms_enabled: false,
            direct_rom_allowed_systems: vec!["snes".to_string()],
            save_states_enabled: false,
            temporary_rom_max_bytes: 1024,
            temporary_roms_enabled: true,
        };

        assert!(policy.can_relay_temporary_roms(&enabled_broker));
        assert!(!policy.can_relay_temporary_roms(&disabled_broker));
        assert!(!policy.can_relay_save_states(&enabled_broker));
    }

    #[test]
    fn direct_rom_relay_capability_allows_android_direct_invite_systems() {
        let policy = FileRelayPolicy {
            direct_roms_enabled: true,
            direct_rom_allowed_systems: vec!["snes".to_string()],
            save_states_enabled: false,
            temporary_rom_max_bytes: 1024,
            temporary_roms_enabled: false,
        };
        let descriptor = direct_rom_descriptor("snes", "snes9x", 512);

        let capability = policy
            .direct_rom_relay_capability(&EnabledFileRelayBroker, &descriptor)
            .expect("capability");

        assert!(capability.supported);
        assert!(capability.available);
        assert!(capability.temporary_access_only);
        assert_eq!(capability.max_bytes, 1024);
        assert_eq!(capability.allowed_systems, vec!["snes"]);
        assert_eq!(capability.reason, None);
    }

    #[test]
    fn direct_rom_relay_capability_blocks_n64_until_policy_allows_it() {
        let policy = FileRelayPolicy {
            direct_roms_enabled: true,
            direct_rom_allowed_systems: vec!["snes".to_string()],
            save_states_enabled: false,
            temporary_rom_max_bytes: 1024,
            temporary_roms_enabled: false,
        };
        let descriptor = direct_rom_descriptor("n64", "mupen64plus-next", 512);

        let capability = policy
            .direct_rom_relay_capability(&EnabledFileRelayBroker, &descriptor)
            .expect("capability");

        assert!(capability.supported);
        assert!(!capability.available);
        assert_eq!(
            capability.reason,
            Some(RomRelayCapabilityReason::UnsupportedSystem)
        );
    }

    fn direct_rom_descriptor(
        system: &str,
        core_id: &str,
        size_bytes: u64,
    ) -> NetplaySessionDescriptor {
        serde_json::from_value(json!({
            "hostClientKind": "android",
            "roomMode": "directInvite",
            "romRelayIntent": "missingPeerOnly",
            "game": {
                "systemId": system,
                "title": "Relay Test",
                "romSha256": "a".repeat(64),
                "contentKey": format!("{system}-relay-test")
            },
            "core": {
                "coreId": core_id,
                "stateFormat": format!("{core_id}:{system}:state-v1")
            },
            "romIdentity": {
                "system": system,
                "coreId": core_id,
                "contentHash": "a".repeat(64),
                "sizeBytes": size_bytes,
                "fileName": format!("Relay Test.{system}"),
                "extension": system,
                "displayName": "Relay Test"
            }
        }))
        .expect("descriptor")
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
