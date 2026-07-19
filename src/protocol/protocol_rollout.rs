//! Platform-scoped protocol rollout policy.
//!
//! The relay can understand both v4 and v5 while requiring newer protocol
//! support from one app family at a time during a staged client rollout.

use crate::protocol::{
    LEGACY_NETPLAY_PROTOCOL_VERSION, MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION,
    NETPLAY_PROTOCOL_VERSION, NetplayClientKind, ProtocolVersionError,
};

/// Server-side creation/join policy layered over the implemented wire range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetplayProtocolRolloutPolicy {
    v5_enabled: bool,
    minimum_android: u16,
    minimum_ios: u16,
    minimum_desktop: u16,
}

impl NetplayProtocolRolloutPolicy {
    /// Builds a validated rollout policy from deployment configuration.
    pub fn new(
        v5_enabled: bool,
        minimum_android: u16,
        minimum_ios: u16,
        minimum_desktop: u16,
    ) -> Result<Self, ProtocolVersionError> {
        let maximum = if v5_enabled {
            NETPLAY_PROTOCOL_VERSION
        } else {
            LEGACY_NETPLAY_PROTOCOL_VERSION
        };
        for minimum in [minimum_android, minimum_ios, minimum_desktop] {
            if !(MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION..=maximum).contains(&minimum) {
                return Err(ProtocolVersionError { version: minimum });
            }
        }

        Ok(Self {
            v5_enabled,
            minimum_android,
            minimum_ios,
            minimum_desktop,
        })
    }

    /// Selects the newest enabled protocol in the client and platform ranges.
    pub fn negotiate(
        &self,
        client_kind: NetplayClientKind,
        client_minimum: u16,
        client_maximum: u16,
    ) -> Result<u16, ProtocolVersionError> {
        if client_minimum > client_maximum {
            return Err(ProtocolVersionError {
                version: client_maximum,
            });
        }
        let minimum = client_minimum.max(self.minimum_for(client_kind));
        let maximum = client_maximum.min(self.maximum());
        if minimum > maximum {
            return Err(ProtocolVersionError {
                version: client_maximum,
            });
        }
        Ok(maximum)
    }

    /// Validates one exact lobby or room protocol for an authenticated client.
    pub fn validate_exact(
        &self,
        client_kind: NetplayClientKind,
        version: u16,
    ) -> Result<(), ProtocolVersionError> {
        if (self.minimum_for(client_kind)..=self.maximum()).contains(&version) {
            Ok(())
        } else {
            Err(ProtocolVersionError { version })
        }
    }

    /// Returns whether protocol-v5 room creation is currently enabled.
    pub fn v5_enabled(&self) -> bool {
        self.v5_enabled
    }

    fn minimum_for(&self, client_kind: NetplayClientKind) -> u16 {
        match client_kind {
            NetplayClientKind::Android => self.minimum_android,
            NetplayClientKind::Ios => self.minimum_ios,
            NetplayClientKind::Desktop => self.minimum_desktop,
        }
    }

    fn maximum(&self) -> u16 {
        if self.v5_enabled {
            NETPLAY_PROTOCOL_VERSION
        } else {
            LEGACY_NETPLAY_PROTOCOL_VERSION
        }
    }
}

impl Default for NetplayProtocolRolloutPolicy {
    fn default() -> Self {
        Self {
            v5_enabled: true,
            minimum_android: MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION,
            minimum_ios: MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION,
            minimum_desktop: MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NetplayProtocolRolloutPolicy;
    use crate::protocol::NetplayClientKind;

    #[test]
    fn platform_minimum_can_move_without_disabling_legacy_desktop() {
        let policy = NetplayProtocolRolloutPolicy::new(true, 5, 4, 4).expect("policy");

        assert!(
            policy
                .validate_exact(NetplayClientKind::Android, 4)
                .is_err()
        );
        assert!(policy.validate_exact(NetplayClientKind::Android, 5).is_ok());
        assert!(policy.validate_exact(NetplayClientKind::Desktop, 4).is_ok());
    }

    #[test]
    fn kill_switch_prevents_v5_negotiation() {
        let policy = NetplayProtocolRolloutPolicy::new(false, 4, 4, 4).expect("policy");

        assert_eq!(policy.negotiate(NetplayClientKind::Android, 4, 5), Ok(4));
        assert!(
            policy
                .validate_exact(NetplayClientKind::Android, 5)
                .is_err()
        );
    }
}
