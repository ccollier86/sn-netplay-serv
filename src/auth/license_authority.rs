//! License authority abstraction.
//!
//! Room and HTTP code depend on this trait instead of a concrete HTTP client so
//! tests can provide deterministic fakes.

use crate::auth::{AuthError, DesktopAuthProof, VerifiedLicense};

/// Verifies whether a ShadowBoy Desktop token can use a server feature.
#[async_trait::async_trait]
pub trait LicenseAuthority: Send + Sync {
    /// Validates `auth` for `feature` and returns the verified license subject.
    async fn verify_desktop_access(
        &self,
        auth: DesktopAuthProof,
        feature: &'static str,
    ) -> Result<VerifiedLicense, AuthError>;
}
