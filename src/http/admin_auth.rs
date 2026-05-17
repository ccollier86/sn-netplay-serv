//! Admin endpoint authorization.
//!
//! Internal observability endpoints require a separate bearer token. The token
//! is optional in config so local development can leave admin routes disabled.

use crate::http::errors::HttpError;
use axum::http::{HeaderMap, header};

/// Verifies access to internal HTTP endpoints.
#[derive(Clone)]
pub struct AdminAuthorizer {
    token: Option<String>,
}

impl AdminAuthorizer {
    /// Creates an authorizer from an optional configured token.
    pub fn new(token: Option<String>) -> Self {
        Self { token }
    }

    /// Verifies the request `Authorization` header.
    pub fn verify(&self, headers: &HeaderMap) -> Result<(), HttpError> {
        let expected = self.token.as_deref().ok_or(HttpError::AdminDisabled)?;
        let header = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim);

        match header {
            Some(actual) if constant_time_eq(actual.as_bytes(), expected.as_bytes()) => Ok(()),
            _ => Err(HttpError::AdminUnauthorized),
        }
    }
}

impl std::fmt::Debug for AdminAuthorizer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdminAuthorizer")
            .field("enabled", &self.token.is_some())
            .finish()
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right.iter())
        .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::AdminAuthorizer;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn disabled_authorizer_rejects_requests() {
        let authorizer = AdminAuthorizer::new(None);

        assert!(authorizer.verify(&HeaderMap::new()).is_err());
    }

    #[test]
    fn accepts_matching_bearer_token() {
        let authorizer = AdminAuthorizer::new(Some("secret".to_string()));
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer secret"));

        assert!(authorizer.verify(&headers).is_ok());
    }
}
