//! Auth-specific error types.
//!
//! These errors avoid embedding raw tokens or secrets so they can be safely
//! mapped to HTTP and protocol responses.

/// Failure while validating a desktop license token.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// No bearer token was supplied.
    #[error("missing desktop token")]
    MissingToken,
    /// No install id was supplied.
    #[error("missing desktop installation id")]
    MissingInstallationId,
    /// License authority rejected the supplied token or feature.
    #[error("license is not authorized for netplay")]
    Unauthorized,
    /// The install is authenticated but lacks an active netplay entitlement.
    #[error("premium or active trial entitlement is required for netplay")]
    EntitlementRequired,
    /// License authority returned a response the server could not use.
    #[error("invalid license authority response")]
    InvalidAuthorityResponse,
    /// License authority endpoint URL was invalid.
    #[error("invalid license authority URL")]
    InvalidAuthorityUrl,
    /// Network or protocol failure while calling the license authority.
    #[error("license authority request failed")]
    AuthorityRequestFailed,
}
