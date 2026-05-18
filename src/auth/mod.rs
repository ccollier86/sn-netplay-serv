//! Protected-client verification boundary for netplay access.
//!
//! Auth code owns communication with the existing ShadowBoy license authority.
//! It does not create rooms, inspect protocol messages, or log sensitive
//! client tokens.

mod authority_response;
mod client_kind;
mod errors;
mod http_license_authority;
mod license_authority;
mod protected_client_auth_proof;
mod verified_license;

pub(crate) use authority_response::parse_verified_license;
pub use client_kind::ClientKind;
pub use errors::AuthError;
pub use http_license_authority::HttpLicenseAuthority;
pub use license_authority::LicenseAuthority;
pub use protected_client_auth_proof::{
    ProtectedAccessToken, ProtectedClientAuthProof, ProtectedInstallationId, ProtectedRequestProof,
};
pub use verified_license::VerifiedLicense;
