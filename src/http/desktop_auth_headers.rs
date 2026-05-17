//! Desktop protected-auth header parsing for HTTP and WebSocket routes.
//!
//! This module translates inbound relay request headers into the auth proof
//! sent to the metadata service. It does not verify licenses or mutate rooms.

use crate::auth::{
    DesktopAuthProof, DesktopInstallationId, DesktopProtectedRequestProof, DesktopToken,
};
use crate::http::errors::HttpError;
use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

/// Builds a Desktop auth proof from relay request headers.
pub fn desktop_auth_proof(
    headers: &HeaderMap,
    method: &str,
    path_and_query: &str,
    body: &[u8],
) -> Result<DesktopAuthProof, HttpError> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .ok_or(crate::auth::AuthError::MissingToken)?;
    let value = header
        .to_str()
        .map_err(|_| crate::auth::AuthError::MissingToken)?;
    let token = value
        .strip_prefix("Bearer ")
        .ok_or(crate::auth::AuthError::MissingToken)?;
    let access_token = DesktopToken::new(token)?;
    let installation_id = DesktopInstallationId::new(required_header(headers, "x-install-id")?)?;

    Ok(DesktopAuthProof::new(
        access_token,
        installation_id,
        DesktopProtectedRequestProof {
            method: method.to_string(),
            path_and_query: path_and_query.to_string(),
            body_sha256_hex: format!("{:x}", Sha256::digest(body)),
            nonce: optional_header(headers, "x-req-nonce"),
            signature: optional_header(headers, "x-req-sig"),
            timestamp: optional_header(headers, "x-req-ts"),
        },
    ))
}

fn required_header(headers: &HeaderMap, name: &'static str) -> Result<String, HttpError> {
    let value = headers
        .get(name)
        .ok_or(crate::auth::AuthError::MissingInstallationId)?
        .to_str()
        .map_err(|_| crate::auth::AuthError::MissingInstallationId)?
        .trim()
        .to_string();

    if value.is_empty() {
        Err(crate::auth::AuthError::MissingInstallationId.into())
    } else {
        Ok(value)
    }
}

fn optional_header(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}
