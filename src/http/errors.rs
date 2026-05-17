//! HTTP error mapping.
//!
//! This module maps auth and room failures to stable JSON responses without
//! exposing raw tokens, secrets, or internal transport details.

use crate::auth::AuthError;
use crate::rooms::RoomError;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Error returned by an HTTP route.
#[derive(Debug)]
pub enum HttpError {
    /// License validation failed.
    Auth(AuthError),
    /// Room domain operation failed.
    Room(RoomError),
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

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
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
        };

        (status, Json(ErrorBody { code, message })).into_response()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}
