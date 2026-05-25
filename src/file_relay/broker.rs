//! File relay broker abstraction.
//!
//! The netplay server uses this trait to ask the trusted file-relay service for
//! temporary transfer tickets without depending on an HTTP client in lobby code.

use crate::file_relay::{CreateFileRelayTransferRequest, CreateFileRelayTransferResponse};

/// Trusted file relay behavior needed by lobby orchestration.
#[async_trait::async_trait]
pub trait FileRelayBroker: Send + Sync {
    /// Returns whether this broker can create transfer tickets.
    fn is_enabled(&self) -> bool;

    /// Creates a temporary transfer ticket.
    async fn create_transfer(
        &self,
        request: CreateFileRelayTransferRequest,
    ) -> Result<CreateFileRelayTransferResponse, FileRelayBrokerError>;
}

/// Broker used when file relay integration is not configured.
#[derive(Default)]
pub struct DisabledFileRelayBroker;

#[async_trait::async_trait]
impl FileRelayBroker for DisabledFileRelayBroker {
    fn is_enabled(&self) -> bool {
        false
    }

    async fn create_transfer(
        &self,
        _request: CreateFileRelayTransferRequest,
    ) -> Result<CreateFileRelayTransferResponse, FileRelayBrokerError> {
        Err(FileRelayBrokerError::Disabled)
    }
}

/// Failure from the trusted file relay.
#[derive(Debug, thiserror::Error)]
pub enum FileRelayBrokerError {
    /// File relay integration is disabled.
    #[error("file relay broker is disabled")]
    Disabled,
    /// Configured broker URL is invalid.
    #[error("file relay broker url is invalid")]
    InvalidUrl,
    /// HTTP request to the broker failed.
    #[error("file relay broker request failed")]
    RequestFailed,
    /// Broker rejected or failed the request.
    #[error("file relay broker returned status {0}")]
    UnexpectedStatus(u16),
    /// Broker response JSON was not usable.
    #[error("file relay broker response was invalid")]
    InvalidResponse,
}

#[cfg(test)]
mod tests {
    use super::{DisabledFileRelayBroker, FileRelayBroker, FileRelayBrokerError};
    use crate::file_relay::{CreateFileRelayTransferRequest, FileRelayTransferKind};

    #[tokio::test]
    async fn disabled_broker_rejects_transfer_creation() {
        let broker = DisabledFileRelayBroker;
        let request = CreateFileRelayTransferRequest {
            room_id: "room-1".to_string(),
            sender_player_id: "p1".to_string(),
            receiver_player_id: "p2".to_string(),
            kind: FileRelayTransferKind::Rom,
            sha256: "a".repeat(64),
            size_bytes: 128,
            expires_in_seconds: None,
        };
        let error = broker.create_transfer(request).await.expect_err("disabled");

        assert!(!broker.is_enabled());
        assert!(matches!(error, FileRelayBrokerError::Disabled));
    }
}
