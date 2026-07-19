//! Protected-client auth header parsing for HTTP and WebSocket routes.
//!
//! This module translates inbound relay request headers into the auth proof
//! sent to the metadata service. It does not verify licenses or mutate rooms.

use crate::auth::{
    ClientKind, ProtectedAccessToken, ProtectedClientAuthProof, ProtectedInstallationId,
    ProtectedRequestProof,
};
use crate::http::errors::HttpError;
use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

/// Builds a protected-client auth proof from relay request headers.
pub fn client_auth_proof(
    headers: &HeaderMap,
    method: &str,
    path_and_query: &str,
    body: &[u8],
) -> Result<ProtectedClientAuthProof, HttpError> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .ok_or(crate::auth::AuthError::MissingToken)?;
    let value = header
        .to_str()
        .map_err(|_| crate::auth::AuthError::MissingToken)?;
    let token = value
        .strip_prefix("Bearer ")
        .ok_or(crate::auth::AuthError::MissingToken)?;
    let client_kind = client_kind(headers)?;
    let access_token = ProtectedAccessToken::new(token)?;
    let installation_id = ProtectedInstallationId::new(required_installation_id(headers)?)?;

    Ok(ProtectedClientAuthProof::new(
        client_kind,
        access_token,
        installation_id,
        ProtectedRequestProof {
            method: method.to_string(),
            path_and_query: path_and_query.to_string(),
            body_sha256_hex: format!("{:x}", Sha256::digest(body)),
            nonce: optional_header(headers, "x-req-nonce"),
            signature: optional_header(headers, "x-req-sig"),
            timestamp: optional_header(headers, "x-req-ts"),
            app_attest_key_id: optional_header(headers, "x-app-attest-key-id"),
            app_attest_assertion: optional_header(headers, "x-app-attest-assertion"),
        },
    ))
}

fn client_kind(headers: &HeaderMap) -> Result<ClientKind, HttpError> {
    match optional_header(headers, "x-client-kind")
        .or_else(|| optional_header(headers, "x-shadowboy-client-kind"))
    {
        Some(value) => ClientKind::parse(&value).map_err(Into::into),
        None => Ok(ClientKind::Desktop),
    }
}

fn required_installation_id(headers: &HeaderMap) -> Result<String, HttpError> {
    optional_header(headers, "x-install-id")
        .or_else(|| optional_header(headers, "x-installation-id"))
        .ok_or_else(|| crate::auth::AuthError::MissingInstallationId.into())
}

fn optional_header(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::client_auth_proof;
    use crate::auth::ClientKind;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn defaults_to_desktop_for_existing_clients() {
        let mut headers = valid_headers();

        let proof = client_auth_proof(&headers, "GET", "/v1/ws", &[]).expect("proof");

        assert_eq!(proof.client_kind, ClientKind::Desktop);
        headers.clear();
    }

    #[test]
    fn accepts_android_client_kind() {
        let mut headers = valid_headers();
        headers.insert("x-client-kind", HeaderValue::from_static("android"));

        let proof = client_auth_proof(&headers, "GET", "/v1/ws", &[]).expect("proof");

        assert_eq!(proof.client_kind, ClientKind::Android);
    }

    #[test]
    fn accepts_ios_client_kind_and_app_attest_proof() {
        let mut headers = valid_headers();
        headers.insert("x-client-kind", HeaderValue::from_static("ios"));
        headers.insert(
            "x-app-attest-key-id",
            HeaderValue::from_static("app-attest-key"),
        );
        headers.insert(
            "x-app-attest-assertion",
            HeaderValue::from_static("app-attest-assertion"),
        );

        let proof =
            client_auth_proof(&headers, "GET", "/v1/lobbies/public/ws", &[]).expect("proof");

        assert_eq!(proof.client_kind, ClientKind::Ios);
        assert_eq!(
            proof.request.app_attest_key_id.as_deref(),
            Some("app-attest-key")
        );
        assert_eq!(
            proof.request.app_attest_assertion.as_deref(),
            Some("app-attest-assertion")
        );
    }

    #[test]
    fn accepts_installation_id_alias() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer valid"));
        headers.insert("x-installation-id", HeaderValue::from_static("install-1"));

        let proof = client_auth_proof(&headers, "GET", "/v1/ws", &[]).expect("proof");

        assert_eq!(proof.installation_id.as_str(), "install-1");
    }

    fn valid_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer valid"));
        headers.insert("x-install-id", HeaderValue::from_static("install-1"));
        headers
    }
}
