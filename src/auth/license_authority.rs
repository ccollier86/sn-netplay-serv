//! License authority abstraction.
//!
//! Room and HTTP code depend on this trait instead of a concrete HTTP client so
//! tests can provide deterministic fakes.

use crate::auth::{AuthError, ProtectedClientAuthProof, VerifiedLicense};

/// Verifies whether a ShadowBoy client token can use a server feature.
#[async_trait::async_trait]
pub trait LicenseAuthority: Send + Sync {
    /// Validates `auth` for `feature` and returns the verified license subject.
    async fn verify_client_access(
        &self,
        auth: ProtectedClientAuthProof,
        feature: &'static str,
    ) -> Result<VerifiedLicense, AuthError>;
}
