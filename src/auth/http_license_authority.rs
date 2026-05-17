//! HTTP client for the existing ShadowBoy license authority.
//!
//! This module owns outbound metadata-service calls. It does not know room
//! state, player slots, or WebSocket protocol details.

use crate::auth::{
    AuthError, DesktopAuthProof, LicenseAuthority, VerifiedLicense, parse_verified_license,
};
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;

/// License verifier backed by the ShadowBoy metadata/cheat service.
pub struct HttpLicenseAuthority {
    client: reqwest::Client,
    verify_url: Url,
    internal_secret: String,
}

impl HttpLicenseAuthority {
    /// Creates an HTTP-backed license authority client.
    pub fn new(
        verify_url: impl AsRef<str>,
        internal_secret: impl Into<String>,
    ) -> Result<Self, AuthError> {
        let client = reqwest::Client::new();
        let verify_url =
            Url::parse(verify_url.as_ref()).map_err(|_| AuthError::InvalidAuthorityUrl)?;

        Ok(Self {
            client,
            verify_url,
            internal_secret: internal_secret.into(),
        })
    }
}

#[async_trait::async_trait]
impl LicenseAuthority for HttpLicenseAuthority {
    async fn verify_desktop_access(
        &self,
        auth: DesktopAuthProof,
        feature: &'static str,
    ) -> Result<VerifiedLicense, AuthError> {
        let response = self
            .client
            .post(self.verify_url.clone())
            .bearer_auth(&self.internal_secret)
            .json(&VerifyDesktopAccessRequest {
                access_token: auth.access_token.expose_secret(),
                feature,
                installation_id: auth.installation_id.as_str(),
                protected_request: VerifyProtectedRequest {
                    body_sha256_hex: auth.request.body_sha256_hex.as_str(),
                    method: auth.request.method.as_str(),
                    nonce: auth.request.nonce.as_deref(),
                    path_and_query: auth.request.path_and_query.as_str(),
                    signature: auth.request.signature.as_deref(),
                    timestamp: auth.request.timestamp.as_deref(),
                },
                required_entitlement: "premiumOrTrial",
            })
            .send()
            .await
            .map_err(|_| AuthError::AuthorityRequestFailed)?;

        if response.status().as_u16() == 401 {
            return Err(AuthError::Unauthorized);
        }

        if response.status().as_u16() == 402 || response.status().as_u16() == 403 {
            return Err(AuthError::EntitlementRequired);
        }

        if !response.status().is_success() {
            return Err(AuthError::AuthorityRequestFailed);
        }

        let authority_response = response
            .json::<Value>()
            .await
            .map_err(|_| AuthError::InvalidAuthorityResponse)?;

        parse_verified_license(authority_response, auth.installation_id.as_str(), feature)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyDesktopAccessRequest<'a> {
    access_token: &'a str,
    feature: &'static str,
    installation_id: &'a str,
    protected_request: VerifyProtectedRequest<'a>,
    required_entitlement: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyProtectedRequest<'a> {
    body_sha256_hex: &'a str,
    method: &'a str,
    nonce: Option<&'a str>,
    path_and_query: &'a str,
    signature: Option<&'a str>,
    timestamp: Option<&'a str>,
}
