//! Desktop protected-auth proof supplied to the relay.
//!
//! The relay receives the current Desktop access token and install id, then
//! asks the trusted metadata service to authorize netplay. Sensitive values are
//! redacted from debug output.

/// Bearer access token supplied by ShadowBoy Desktop.
#[derive(Clone, Eq, PartialEq)]
pub struct DesktopToken(String);

impl DesktopToken {
    /// Creates a token from a bearer value after trimming surrounding space.
    pub fn new(value: impl Into<String>) -> Result<Self, crate::auth::AuthError> {
        let value = value.into().trim().to_string();
        if value.is_empty() {
            Err(crate::auth::AuthError::MissingToken)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the raw token for the outbound authorization request.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for DesktopToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DesktopToken(<redacted>)")
    }
}

/// Install id from the Desktop protected-client session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopInstallationId(String);

impl DesktopInstallationId {
    /// Creates an install id after trimming surrounding space.
    pub fn new(value: impl Into<String>) -> Result<Self, crate::auth::AuthError> {
        let value = value.into().trim().to_string();
        if value.is_empty() {
            Err(crate::auth::AuthError::MissingInstallationId)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the install id used by the metadata service.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Original protected request details for backend signature verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopProtectedRequestProof {
    /// HTTP method used for the netplay request.
    pub method: String,
    /// Path and query signed by Desktop.
    pub path_and_query: String,
    /// SHA-256 hex digest of the exact request body bytes.
    pub body_sha256_hex: String,
    /// Request nonce from `X-Req-Nonce`, when supplied.
    pub nonce: Option<String>,
    /// ECDSA signature from `X-Req-Sig`, when supplied.
    pub signature: Option<String>,
    /// Request timestamp from `X-Req-Ts`, when supplied.
    pub timestamp: Option<String>,
}

/// Auth proof forwarded from Desktop to the relay.
#[derive(Clone, Eq, PartialEq)]
pub struct DesktopAuthProof {
    /// Current protected-client access token.
    pub access_token: DesktopToken,
    /// Install id tied to the access token.
    pub installation_id: DesktopInstallationId,
    /// Signed netplay request details.
    pub request: DesktopProtectedRequestProof,
}

impl DesktopAuthProof {
    /// Creates a protected-auth proof for the metadata authorization endpoint.
    pub fn new(
        access_token: DesktopToken,
        installation_id: DesktopInstallationId,
        request: DesktopProtectedRequestProof,
    ) -> Self {
        Self {
            access_token,
            installation_id,
            request,
        }
    }
}

impl std::fmt::Debug for DesktopAuthProof {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesktopAuthProof")
            .field("access_token", &"<redacted>")
            .field("installation_id", &self.installation_id)
            .field("request", &self.request)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopAuthProof, DesktopInstallationId, DesktopProtectedRequestProof, DesktopToken,
    };

    #[test]
    fn token_debug_output_redacts_secret() {
        let token = DesktopToken::new("secret-token").expect("token");

        assert_eq!(format!("{token:?}"), "DesktopToken(<redacted>)");
    }

    #[test]
    fn auth_proof_debug_output_redacts_token() {
        let proof = DesktopAuthProof::new(
            DesktopToken::new("secret-token").expect("token"),
            DesktopInstallationId::new("install-1").expect("install id"),
            DesktopProtectedRequestProof {
                method: "POST".to_string(),
                path_and_query: "/v1/rooms".to_string(),
                body_sha256_hex: "hash".to_string(),
                nonce: None,
                signature: None,
                timestamp: None,
            },
        );

        assert!(!format!("{proof:?}").contains("secret-token"));
    }
}
