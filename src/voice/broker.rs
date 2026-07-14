//! Voice broker abstraction.
//!
//! Room lifecycle code depends on this trait instead of a concrete HTTP client,
//! which keeps tests and future service-auth changes isolated.

use crate::voice::{
    CreateVoiceRoomBrokerRequest, CreateVoiceRoomBrokerResponse, IssueVoiceTokenBrokerRequest,
    VoiceBrokerGrant,
};

/// Trusted broker behavior needed by the netplay relay.
#[async_trait::async_trait]
pub trait VoiceBroker: Send + Sync {
    /// Returns whether this broker can create voice rooms.
    fn is_enabled(&self) -> bool;

    /// Creates a provider-backed voice room for one netplay room.
    async fn create_room(
        &self,
        request: CreateVoiceRoomBrokerRequest,
    ) -> Result<CreateVoiceRoomBrokerResponse, VoiceBrokerError>;

    /// Issues a fresh voice token for an existing room.
    async fn issue_token(
        &self,
        voice_room_id: &str,
        request: IssueVoiceTokenBrokerRequest,
    ) -> Result<VoiceBrokerGrant, VoiceBrokerError>;

    /// Closes a provider-backed voice room.
    async fn close_room(&self, voice_room_id: &str, reason: &str) -> Result<(), VoiceBrokerError>;

    /// Disconnects one participant's active provider session.
    async fn remove_participant(
        &self,
        voice_room_id: &str,
        participant_identity: &str,
        reason: &str,
    ) -> Result<(), VoiceBrokerError>;
}

/// Broker used when voice integration is not configured.
#[derive(Default)]
pub struct DisabledVoiceBroker;

#[async_trait::async_trait]
impl VoiceBroker for DisabledVoiceBroker {
    fn is_enabled(&self) -> bool {
        false
    }

    async fn create_room(
        &self,
        _request: CreateVoiceRoomBrokerRequest,
    ) -> Result<CreateVoiceRoomBrokerResponse, VoiceBrokerError> {
        Err(VoiceBrokerError::Disabled)
    }

    async fn issue_token(
        &self,
        _voice_room_id: &str,
        _request: IssueVoiceTokenBrokerRequest,
    ) -> Result<VoiceBrokerGrant, VoiceBrokerError> {
        Err(VoiceBrokerError::Disabled)
    }

    async fn close_room(
        &self,
        _voice_room_id: &str,
        _reason: &str,
    ) -> Result<(), VoiceBrokerError> {
        Ok(())
    }

    async fn remove_participant(
        &self,
        _voice_room_id: &str,
        _participant_identity: &str,
        _reason: &str,
    ) -> Result<(), VoiceBrokerError> {
        Ok(())
    }
}

/// Failure from the trusted voice broker.
#[derive(Debug, thiserror::Error)]
pub enum VoiceBrokerError {
    /// Voice broker integration is disabled.
    #[error("voice broker is disabled")]
    Disabled,
    /// Configured broker URL is invalid.
    #[error("voice broker url is invalid")]
    InvalidUrl,
    /// HTTP request to the broker failed.
    #[error("voice broker request failed")]
    RequestFailed,
    /// Broker rejected or failed the request.
    #[error("voice broker returned status {0}")]
    UnexpectedStatus(u16),
    /// Broker response JSON was not usable.
    #[error("voice broker response was invalid")]
    InvalidResponse,
}
