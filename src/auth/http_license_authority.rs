//! HTTP client for the existing ShadowBoy license authority.
//!
//! This module owns outbound metadata-service calls. It does not know room
//! state, player slots, or WebSocket protocol details.

use crate::auth::{
    AuthError, LicenseAuthority, ProtectedClientAuthProof, VerifiedLicense, parse_verified_license,
};
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

const AUTHORITY_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const AUTHORITY_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

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
        let client = reqwest::Client::builder()
            .connect_timeout(AUTHORITY_CONNECT_TIMEOUT)
            .timeout(AUTHORITY_REQUEST_TIMEOUT)
            .build()
            .map_err(|_| AuthError::AuthorityRequestFailed)?;
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
    async fn verify_client_access(
        &self,
        auth: ProtectedClientAuthProof,
        feature: &'static str,
    ) -> Result<VerifiedLicense, AuthError> {
        let response = self
            .client
            .post(self.verify_url.clone())
            .bearer_auth(&self.internal_secret)
            .json(&VerifyClientAccessRequest {
                access_token: auth.access_token.expose_secret(),
                client_kind: auth.client_kind.as_str(),
                feature,
                installation_id: auth.installation_id.as_str(),
                protected_request: VerifyProtectedRequest {
                    body_sha256_hex: auth.request.body_sha256_hex.as_str(),
                    method: auth.request.method.as_str(),
                    nonce: auth.request.nonce.as_deref(),
                    path_and_query: auth.request.path_and_query.as_str(),
                    signature: auth.request.signature.as_deref(),
                    timestamp: auth.request.timestamp.as_deref(),
                    app_attest_key_id: auth.request.app_attest_key_id.as_deref(),
                    app_attest_assertion: auth.request.app_attest_assertion.as_deref(),
                },
                required_entitlement: auth.client_kind.required_entitlement(),
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

        parse_verified_license(
            authority_response,
            auth.client_kind,
            auth.installation_id.as_str(),
            feature,
        )
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyClientAccessRequest<'a> {
    access_token: &'a str,
    client_kind: &'static str,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    app_attest_key_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_attest_assertion: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::{VerifyClientAccessRequest, VerifyProtectedRequest};
    use crate::auth::ClientKind;
    use serde_json::json;

    #[test]
    fn android_authority_request_uses_eligible_client_entitlement() {
        let value = serde_json::to_value(VerifyClientAccessRequest {
            access_token: "token",
            client_kind: ClientKind::Android.as_str(),
            feature: "netplay",
            installation_id: "android-install-1",
            protected_request: VerifyProtectedRequest {
                body_sha256_hex: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                method: "GET",
                nonce: Some("nonce-1"),
                path_and_query: "/v1/ws?inviteCode=AB23-CD&role=guest&protocolVersion=1",
                signature: Some("signature-1"),
                timestamp: Some("2026-05-17T18:30:00Z"),
                app_attest_key_id: None,
                app_attest_assertion: None,
            },
            required_entitlement: ClientKind::Android.required_entitlement(),
        })
        .expect("json");

        assert_eq!(
            value,
            json!({
                "accessToken": "token",
                "clientKind": "android",
                "feature": "netplay",
                "installationId": "android-install-1",
                "protectedRequest": {
                    "bodySha256Hex": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    "method": "GET",
                    "nonce": "nonce-1",
                    "pathAndQuery": "/v1/ws?inviteCode=AB23-CD&role=guest&protocolVersion=1",
                    "signature": "signature-1",
                    "timestamp": "2026-05-17T18:30:00Z"
                },
                "requiredEntitlement": "eligibleClient"
            })
        );
    }

    #[test]
    fn desktop_authority_request_keeps_premium_or_trial_entitlement() {
        let value = serde_json::to_value(VerifyClientAccessRequest {
            access_token: "token",
            client_kind: ClientKind::Desktop.as_str(),
            feature: "netplay",
            installation_id: "desktop-install-1",
            protected_request: VerifyProtectedRequest {
                body_sha256_hex: "hash",
                method: "POST",
                nonce: None,
                path_and_query: "/v1/rooms",
                signature: None,
                timestamp: None,
                app_attest_key_id: None,
                app_attest_assertion: None,
            },
            required_entitlement: ClientKind::Desktop.required_entitlement(),
        })
        .expect("json");

        assert_eq!(value["clientKind"], "desktop");
        assert_eq!(value["requiredEntitlement"], "premiumOrTrial");
    }

    #[test]
    fn ios_authority_request_forwards_app_attest_proof() {
        let value = serde_json::to_value(VerifyClientAccessRequest {
            access_token: "token",
            client_kind: ClientKind::Ios.as_str(),
            feature: "netplay",
            installation_id: "ios-install-1",
            protected_request: VerifyProtectedRequest {
                body_sha256_hex: "hash",
                method: "GET",
                nonce: Some("nonce"),
                path_and_query: "/v1/lobbies/public/ws",
                signature: None,
                timestamp: Some("1784069961000"),
                app_attest_key_id: Some("app-attest-key"),
                app_attest_assertion: Some("app-attest-assertion"),
            },
            required_entitlement: ClientKind::Ios.required_entitlement(),
        })
        .expect("json");

        assert_eq!(value["clientKind"], "ios");
        assert_eq!(value["requiredEntitlement"], "eligibleClient");
        assert_eq!(
            value["protectedRequest"]["appAttestKeyId"],
            "app-attest-key"
        );
        assert_eq!(
            value["protectedRequest"]["appAttestAssertion"],
            "app-attest-assertion"
        );
        assert!(value["protectedRequest"]["signature"].is_null());
    }
}
