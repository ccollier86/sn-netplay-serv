//! Private link data-plane grants sent only to one authenticated room endpoint.
//!
//! These values deliberately stay out of `RoomView`, controller messages, room
//! events, and debug history. `roomScope` is control-plane admission metadata
//! and is never copied into an SBLK frame.

use serde::Serialize;
use std::fmt;

/// Availability of one private link data-plane generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkCableGrantStatus {
    /// Only one authenticated endpoint is currently attached.
    WaitingForPeer,
    /// The current generation can accept validated SBLK events.
    Ready,
    /// The generation failed closed and requires a newer cable epoch.
    Aborted,
    /// The owning link provider has closed permanently.
    Closed,
}

/// Safe reason associated with a failed or closed private link route.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkCableGrantFailureReason {
    /// Provider state changed before the cable generation could continue.
    ProviderReset,
    /// The other authenticated endpoint detached.
    PeerDisconnected,
    /// A required event could not enter the bounded peer queue.
    QueueOverflow,
    /// A frame violated the negotiated SBLK/session contract.
    ProtocolViolation,
    /// The owning room or provider closed.
    RouteClosed,
}

/// Authenticated private route and native-admission metadata for one endpoint.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCableDataPlaneGrant {
    /// Explicit completed link control/wire contract.
    pub contract_version: u16,
    /// Stable positive decimal namespace allocated once for the authoritative
    /// room id. A string preserves the complete 63-bit value in JavaScript and
    /// every other client runtime.
    pub room_scope: String,
    /// Current authoritative room generation.
    pub room_epoch: u64,
    /// Current authoritative gameplay-provider generation.
    pub session_epoch: u64,
    /// Current server-issued virtual-cable generation, or zero while waiting.
    pub cable_epoch: u64,
    /// Authenticated local lobby slot.
    pub local_slot: u8,
    /// Frozen SBLK body namespace selected by the room descriptor.
    pub link_protocol: String,
    /// Maximum complete SBLK frame bytes accepted by this route.
    pub maximum_event_bytes: u16,
    /// Maximum required events buffered toward either endpoint.
    pub queue_capacity: u16,
    /// Current route availability.
    pub status: LinkCableGrantStatus,
    /// Failure that ended the previous/current generation, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<LinkCableGrantFailureReason>,
}

impl fmt::Debug for LinkCableDataPlaneGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinkCableDataPlaneGrant")
            .field("contract_version", &self.contract_version)
            .field("room_scope", &"<redacted>")
            .field("room_epoch", &self.room_epoch)
            .field("session_epoch", &self.session_epoch)
            .field("cable_epoch", &self.cable_epoch)
            .field("local_slot", &self.local_slot)
            .field("link_protocol", &self.link_protocol)
            .field("maximum_event_bytes", &self.maximum_event_bytes)
            .field("queue_capacity", &self.queue_capacity)
            .field("status", &self.status)
            .field("failure_reason", &self.failure_reason)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{LinkCableDataPlaneGrant, LinkCableGrantStatus};

    #[test]
    fn debug_output_redacts_private_room_scope() {
        let grant = LinkCableDataPlaneGrant {
            contract_version: 1,
            room_scope: "4815162342".to_string(),
            room_epoch: 2,
            session_epoch: 3,
            cable_epoch: 4,
            local_slot: 1,
            link_protocol: "gba-sio-multi-v1".to_string(),
            maximum_event_bytes: 128,
            queue_capacity: 64,
            status: LinkCableGrantStatus::Ready,
            failure_reason: None,
        };

        let debug = format!("{grant:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("4815162342"));
    }

    #[test]
    fn room_scope_serializes_as_an_exact_decimal_string() {
        let grant = LinkCableDataPlaneGrant {
            contract_version: 1,
            room_scope: i64::MAX.to_string(),
            room_epoch: 2,
            session_epoch: 3,
            cable_epoch: 4,
            local_slot: 1,
            link_protocol: "gb-serial-v1".to_string(),
            maximum_event_bytes: 128,
            queue_capacity: 64,
            status: LinkCableGrantStatus::Ready,
            failure_reason: None,
        };

        let json = serde_json::to_value(grant).expect("serialize link grant");
        assert_eq!(json["roomScope"], i64::MAX.to_string());
        assert!(json["roomScope"].is_string());
    }
}
