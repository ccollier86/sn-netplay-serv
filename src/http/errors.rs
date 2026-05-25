//! HTTP error mapping.
//!
//! This module maps auth and room failures to stable JSON responses without
//! exposing raw tokens, secrets, or internal transport details.

use crate::auth::AuthError;
use crate::lobbies::LobbyError;
use crate::protocol::{ProtocolVersionError, SessionDescriptorError};
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
    /// Lobby domain operation failed.
    Lobby(LobbyError),
    /// Request exceeded a configured rate limit.
    RateLimited(RateLimitExceeded),
    /// Client supplied malformed or unsupported request data.
    InvalidRequest {
        /// Stable error code.
        code: &'static str,
        /// Safe user-facing message.
        message: &'static str,
    },
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

impl From<LobbyError> for HttpError {
    fn from(value: LobbyError) -> Self {
        Self::Lobby(value)
    }
}

impl From<RateLimitExceeded> for HttpError {
    fn from(value: RateLimitExceeded) -> Self {
        Self::RateLimited(value)
    }
}

impl From<ProtocolVersionError> for HttpError {
    fn from(_value: ProtocolVersionError) -> Self {
        Self::InvalidRequest {
            code: "unsupportedProtocolVersion",
            message: "This netplay protocol version is not supported.",
        }
    }
}

impl From<SessionDescriptorError> for HttpError {
    fn from(_value: SessionDescriptorError) -> Self {
        Self::InvalidRequest {
            code: "invalidSessionDescriptor",
            message: "Netplay session details are invalid.",
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let mut retry_after_seconds = None;
        let (status, code, message) = match self {
            Self::Auth(AuthError::MissingToken | AuthError::MissingInstallationId) => (
                StatusCode::UNAUTHORIZED,
                "missingClientAuth",
                "Missing client authorization proof.",
            ),
            Self::Auth(AuthError::UnsupportedClientKind) => (
                StatusCode::BAD_REQUEST,
                "unsupportedClientKind",
                "This ShadowBoy client platform is not supported for netplay.",
            ),
            Self::Auth(AuthError::Unauthorized) => (
                StatusCode::FORBIDDEN,
                "notAuthorized",
                "This license is not authorized for netplay.",
            ),
            Self::Auth(AuthError::EntitlementRequired) => (
                StatusCode::PAYMENT_REQUIRED,
                "entitlementRequired",
                "This client is not eligible for netplay.",
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
            Self::Room(RoomError::StaleRoomEpoch) => (
                StatusCode::CONFLICT,
                "staleRoomEpoch",
                "Room state changed; refresh and retry.",
            ),
            Self::Room(RoomError::StaleSessionEpoch) => (
                StatusCode::CONFLICT,
                "staleSessionEpoch",
                "Netplay session changed; refresh and retry.",
            ),
            Self::Room(RoomError::ResumeTokenInvalid) => (
                StatusCode::UNAUTHORIZED,
                "resumeTokenInvalid",
                "Reconnect token is invalid.",
            ),
            Self::Room(RoomError::RecoveryExpired) => (
                StatusCode::GONE,
                "recoveryExpired",
                "Reconnect recovery window expired.",
            ),
            Self::Room(_) => (
                StatusCode::CONFLICT,
                "roomStateConflict",
                "Room state does not allow this operation.",
            ),
            Self::Lobby(LobbyError::NotFound) => (
                StatusCode::NOT_FOUND,
                "lobbyNotFound",
                "Lobby was not found.",
            ),
            Self::Lobby(LobbyError::LobbyFull) => {
                (StatusCode::CONFLICT, "lobbyFull", "Lobby is full.")
            }
            Self::Lobby(LobbyError::LobbyClosed) => {
                (StatusCode::GONE, "lobbyClosed", "Lobby is closed.")
            }
            Self::Lobby(LobbyError::StaleLobbyEpoch) => (
                StatusCode::CONFLICT,
                "staleLobbyEpoch",
                "Lobby state changed; refresh and retry.",
            ),
            Self::Lobby(LobbyError::ResumeTokenInvalid) => (
                StatusCode::UNAUTHORIZED,
                "lobbyResumeTokenInvalid",
                "Lobby reconnect token is invalid.",
            ),
            Self::Lobby(LobbyError::PlayerSlotUnavailable) => (
                StatusCode::CONFLICT,
                "lobbyPlayerSlotUnavailable",
                "Lobby player slot is not available.",
            ),
            Self::Lobby(LobbyError::UnknownConnection) => (
                StatusCode::CONFLICT,
                "unknownLobbyConnection",
                "Connection is not assigned to this lobby.",
            ),
            Self::Lobby(LobbyError::HostOnly) => (
                StatusCode::FORBIDDEN,
                "lobbyHostOnly",
                "Only Player 1 can perform this action.",
            ),
            Self::Lobby(LobbyError::StaleGameProposal) => (
                StatusCode::CONFLICT,
                "staleLobbyGameProposal",
                "Selected game changed; refresh and retry.",
            ),
            Self::Lobby(LobbyError::PlayersNotReady) => (
                StatusCode::CONFLICT,
                "lobbyPlayersNotReady",
                "Players are not ready yet.",
            ),
            Self::Lobby(LobbyError::InvalidPayload) => (
                StatusCode::BAD_REQUEST,
                "invalidLobbyPayload",
                "Lobby payload is invalid.",
            ),
            Self::RateLimited(error) => {
                retry_after_seconds = Some(error.retry_after.as_secs().max(1));
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    "rateLimited",
                    "Too many requests. Please wait and try again.",
                )
            }
            Self::InvalidRequest { code, message } => (StatusCode::BAD_REQUEST, code, message),
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
