//! Supported ShadowBoy client platforms for netplay authorization.

use crate::auth::AuthError;
use serde::Serialize;

/// Platform family requesting relay access.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientKind {
    /// ShadowBoy Desktop protected-client session.
    Desktop,
    /// ShadowBoy Android protected-client session.
    Android,
    /// ShadowBoy iOS protected-client session.
    Ios,
}

impl ClientKind {
    /// Returns the canonical contract value sent to the metadata relay.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }

    /// Returns the entitlement rule requested from the metadata relay.
    pub fn required_entitlement(self) -> &'static str {
        match self {
            Self::Desktop => "premiumOrTrial",
            Self::Android => "eligibleClient",
            Self::Ios => "eligibleClient",
        }
    }

    /// Parses a client-kind value from HTTP headers or authority responses.
    pub fn parse(value: &str) -> Result<Self, AuthError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "desktop" => Ok(Self::Desktop),
            "android" => Ok(Self::Android),
            "ios" => Ok(Self::Ios),
            _ => Err(AuthError::UnsupportedClientKind),
        }
    }

    /// Returns a namespace prefix for room identity comparisons.
    pub fn identity_namespace(self) -> &'static str {
        self.as_str()
    }
}
