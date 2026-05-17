//! License verification boundary for netplay access.
//!
//! Auth code owns communication with the existing ShadowBoy license authority.
//! It does not create rooms, inspect protocol messages, or log sensitive
//! desktop tokens.

mod authority_response;
mod desktop_auth_proof;
mod errors;
mod http_license_authority;
mod license_authority;
mod verified_license;

pub(crate) use authority_response::parse_verified_license;
pub use desktop_auth_proof::{
    DesktopAuthProof, DesktopInstallationId, DesktopProtectedRequestProof, DesktopToken,
};
pub use errors::AuthError;
pub use http_license_authority::HttpLicenseAuthority;
pub use license_authority::LicenseAuthority;
pub use verified_license::VerifiedLicense;
