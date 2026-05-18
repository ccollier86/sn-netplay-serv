//! Protected-client auth proof supplied to the relay.
//!
//! The relay receives the current client access token and install id, then asks
//! the trusted metadata service to authorize netplay. Sensitive values are
//! redacted from debug output.

use crate::auth::{AuthError, ClientKind};

/// Bearer access token supplied by a ShadowBoy client.
#[derive(Clone, Eq, PartialEq)]
pub struct ProtectedAccessToken(String);

impl ProtectedAccessToken {
    /// Creates a token from a bearer value after trimming surrounding space.
    pub fn new(value: impl Into<String>) -> Result<Self, AuthError> {
        let value = value.into().trim().to_string();
        if value.is_empty() {
            Err(AuthError::MissingToken)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the raw token for the outbound authorization request.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ProtectedAccessToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProtectedAccessToken(<redacted>)")
    }
}

/// Install id from a protected-client session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedInstallationId(String);

impl ProtectedInstallationId {
    /// Creates an install id after trimming surrounding space.
    pub fn new(value: impl Into<String>) -> Result<Self, AuthError> {
        let value = value.into().trim().to_string();
        if value.is_empty() {
            Err(AuthError::MissingInstallationId)
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
pub struct ProtectedRequestProof {
    /// HTTP method used for the netplay request.
    pub method: String,
    /// Path and query signed by the client.
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

/// Auth proof forwarded from the relay to the metadata service.
#[derive(Clone, Eq, PartialEq)]
pub struct ProtectedClientAuthProof {
    /// Platform family for the client session.
    pub client_kind: ClientKind,
    /// Current protected-client access token.
    pub access_token: ProtectedAccessToken,
    /// Install id tied to the access token.
    pub installation_id: ProtectedInstallationId,
    /// Signed netplay request details.
    pub request: ProtectedRequestProof,
}

impl ProtectedClientAuthProof {
    /// Creates a protected-auth proof for the metadata authorization endpoint.
    pub fn new(
        client_kind: ClientKind,
        access_token: ProtectedAccessToken,
        installation_id: ProtectedInstallationId,
        request: ProtectedRequestProof,
    ) -> Self {
        Self {
            client_kind,
            access_token,
            installation_id,
            request,
        }
    }
}

impl std::fmt::Debug for ProtectedClientAuthProof {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProtectedClientAuthProof")
            .field("client_kind", &self.client_kind)
            .field("access_token", &"<redacted>")
            .field("installation_id", &self.installation_id)
            .field("request", &self.request)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProtectedAccessToken, ProtectedClientAuthProof, ProtectedInstallationId,
        ProtectedRequestProof,
    };
    use crate::auth::ClientKind;

    #[test]
    fn token_debug_output_redacts_secret() {
        let token = ProtectedAccessToken::new("secret-token").expect("token");

        assert_eq!(format!("{token:?}"), "ProtectedAccessToken(<redacted>)");
    }

    #[test]
    fn auth_proof_debug_output_redacts_token() {
        let proof = ProtectedClientAuthProof::new(
            ClientKind::Android,
            ProtectedAccessToken::new("secret-token").expect("token"),
            ProtectedInstallationId::new("install-1").expect("install id"),
            ProtectedRequestProof {
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
