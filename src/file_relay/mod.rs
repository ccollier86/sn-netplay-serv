//! Trusted file relay integration.
//!
//! The file relay is used only for temporary, session-scoped payloads that are
//! too large or inappropriate for the gameplay WebSocket.

mod broker;
mod config;
mod http_broker;
mod types;

pub use broker::{DisabledFileRelayBroker, FileRelayBroker, FileRelayBrokerError};
pub use config::{FileRelayBrokerConfig, FileRelayConfig, HttpFileRelayBrokerConfig};
pub use http_broker::HttpFileRelayBroker;
pub use types::{
    CreateFileRelayTransferRequest, CreateFileRelayTransferResponse, FileRelayTransferKind,
};
