//! Trusted voice-broker client boundary.
//!
//! The netplay relay only orchestrates voice room lifecycle. It never issues
//! LiveKit tokens itself and never exposes player grants through shared views.

mod broker;
mod http_broker;
mod types;

pub use broker::{DisabledVoiceBroker, VoiceBroker, VoiceBrokerError};
pub use http_broker::HttpVoiceBroker;
pub use types::{
    CloseVoiceRoomRequest, CreateVoiceRoomBrokerRequest, CreateVoiceRoomBrokerResponse,
    IssueVoiceTokenBrokerRequest, RemoveVoiceParticipantRequest, VoiceBrokerGrant,
    VoiceBrokerParticipant, VoiceBrokerRoomView,
};
