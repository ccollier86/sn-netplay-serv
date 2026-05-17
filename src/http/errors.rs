//! HTTP error mapping.
//!
//! This module maps auth and room failures to stable JSON responses without
//! exposing raw tokens, secrets, or internal transport details.

use crate::auth::AuthError;
use crate::rate_limit::RateLimitExceeded;
use crate::rooms::RoomError;
use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Error returned by an HTTP route.
#[derive(Debug)]
pub enum HttpError {
    /// License validation failed.
    Auth(AuthError),
    /// Room domain operation failed.
    Room(RoomError),
    /// Request exceeded a configured rate limit.
    RateLimited(RateLimitExceeded),
    /// Internal admin endpoints were not enabled.
    AdminDisabled,
    /// Internal admin endpoint bearer token was missing or wrong.
    AdminUnauthorized,
}

impl From<AuthError> for HttpError {
    fn from(value: AuthError) -> Self {
        Self::Auth(value)
    }
}

impl From<RoomError> for HttpError {
    fn from(value: RoomError) -> Self {
        Self::Room(value)
    }
}

impl From<RateLimitExceeded> for HttpError {
    fn from(value: RateLimitExceeded) -> Self {
        Self::RateLimited(value)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let mut retry_after_seconds = None;
        let (status, code, message) = match self {
            Self::Auth(AuthError::MissingToken | AuthError::MissingInstallationId) => (
                StatusCode::UNAUTHORIZED,
                "missingDesktopAuth",
                "Missing desktop authorization proof.",
            ),
            Self::Auth(AuthError::Unauthorized) => (
                StatusCode::FORBIDDEN,
                "notAuthorized",
                "This license is not authorized for netplay.",
            ),
            Self::Auth(AuthError::EntitlementRequired) => (
                StatusCode::PAYMENT_REQUIRED,
                "entitlementRequired",
                "Premium or active trial access is required for netplay.",
            ),
            Self::Auth(_) => (
                StatusCode::BAD_GATEWAY,
                "licenseAuthorityUnavailable",
                "License verification is unavailable.",
            ),
            Self::Room(RoomError::InvalidInviteCode) => (
                StatusCode::BAD_REQUEST,
                "invalidInviteCode",
                "Invite code is invalid.",
            ),
            Self::Room(RoomError::NotFound) => {
                (StatusCode::NOT_FOUND, "roomNotFound", "Room was not found.")
            }
            Self::Room(RoomError::RoomFull) => (StatusCode::CONFLICT, "roomFull", "Room is full."),
            Self::Room(RoomError::RoomClosed) => {
                (StatusCode::GONE, "roomClosed", "Room is closed.")
            }
            Self::Room(_) => (
                StatusCode::CONFLICT,
                "roomStateConflict",
                "Room state does not allow this operation.",
            ),
            Self::RateLimited(error) => {
                retry_after_seconds = Some(error.retry_after.as_secs().max(1));
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    "rateLimited",
                    "Too many requests. Please wait and try again.",
                )
            }
            Self::AdminDisabled => (
                StatusCode::NOT_FOUND,
                "notFound",
                "The requested endpoint is not available.",
            ),
            Self::AdminUnauthorized => (
                StatusCode::UNAUTHORIZED,
                "adminUnauthorized",
                "Admin authorization is required.",
            ),
        };

        let mut response = (
            status,
            Json(ErrorBody {
                code,
                message,
                retry_after_seconds,
            }),
        )
            .into_response();

        if let Some(retry_after_seconds) = retry_after_seconds
            && let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }

        response
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_seconds: Option<u64>,
}
