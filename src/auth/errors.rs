//! Auth-specific error types.
//!
//! These errors avoid embedding raw tokens or secrets so they can be safely
//! mapped to HTTP and protocol responses.

/// Failure while validating a protected-client token.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// No bearer token was supplied.
    #[error("missing client token")]
    MissingToken,
    /// No install id was supplied.
    #[error("missing client installation id")]
    MissingInstallationId,
    /// Client-kind header or authority response used an unsupported value.
    #[error("unsupported client kind")]
    UnsupportedClientKind,
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
