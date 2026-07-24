//! Link-cable gameplay provider.
//!
//! This wrapper keeps provisional link state behind the provider boundary. It
//! intentionally does not reuse controller input, snapshots, frame clocks, or
//! coordinated pause state.

use super::link_cable_room_state::LinkCableRoomState;
use crate::limits::LINK_CABLE_EVENT_QUEUE_CAPACITY;
use crate::protocol::{LinkCableCompatibility, LinkCableDescriptor, LinkCableWireProtocol};
use crate::rooms::{
    ConnectionId, LinkCableAttachment, LinkCableDataPlaneError, LinkCableDataPlaneHandle,
    LinkCableDataPlaneSnapshot, PlayerIndex, RoomError, RoomScope,
};

/// Runtime state owned only by link-cable rooms.
#[derive(Clone, Debug)]
pub(crate) struct LinkCableSession {
    state: LinkCableRoomState,
    data_plane: LinkCableDataPlaneHandle,
}

impl LinkCableSession {
    /// Builds the one room-private provider selected by a validated descriptor.
    pub(crate) fn new(
        room_scope: RoomScope,
        link: &LinkCableDescriptor,
        room_epoch: u64,
        session_epoch: u64,
    ) -> Self {
        let protocol = match (link.system_family.as_str(), link.link_protocol.as_str()) {
            ("gb", "gb-serial-v1") => LinkCableWireProtocol::GbSerialV1,
            ("gba", "gba-sio-multi-v1") => LinkCableWireProtocol::GbaSioMultiV1,
            ("gba", "gba-sio-multi-v2") => LinkCableWireProtocol::GbaSioMultiV2,
            _ => panic!("validated link-cable descriptor selected an unsupported wire protocol"),
        };
        let data_plane = LinkCableDataPlaneHandle::new(
            room_scope,
            protocol,
            room_epoch,
            session_epoch,
            LINK_CABLE_EVENT_QUEUE_CAPACITY,
        )
        .expect("validated room epochs and positive link queue capacity");

        Self {
            state: LinkCableRoomState::default(),
            data_plane,
        }
    }

    /// Clears compatibility before a new join cycle.
    pub(crate) fn reset(&mut self) {
        self.state.reset();
    }

    /// Returns a cloneable handle for work performed outside the registry lock.
    pub(crate) fn data_plane_handle(&self) -> LinkCableDataPlaneHandle {
        self.data_plane.clone()
    }

    /// Binds one exact authenticated control connection to its lobby slot.
    pub(crate) fn bind_connection(
        &self,
        player_index: PlayerIndex,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, RoomError> {
        self.data_plane
            .bind_connection(player_index, connection_id)
            .map_err(Self::map_data_plane_error)
    }

    /// Atomically replaces a provisional connection during runner handoff.
    pub(crate) fn replace_connection(
        &self,
        player_index: PlayerIndex,
        previous_connection_id: ConnectionId,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, RoomError> {
        self.data_plane
            .replace_connection(player_index, previous_connection_id, connection_id)
            .map_err(Self::map_data_plane_error)
    }

    /// Claims the one targeted receiver for an already-bound connection.
    pub(crate) fn claim_receiver(
        &self,
        connection_id: ConnectionId,
    ) -> Result<LinkCableAttachment, RoomError> {
        self.data_plane
            .claim_receiver(connection_id)
            .map_err(Self::map_data_plane_error)
    }

    /// Returns server-private lifecycle context for sanitized diagnostics.
    pub(crate) fn diagnostic_snapshot(
        &self,
        local_slot: PlayerIndex,
    ) -> Result<LinkCableDataPlaneSnapshot, RoomError> {
        self.data_plane
            .snapshot(local_slot)
            .map_err(Self::map_data_plane_error)
    }

    /// Invalidates one exact connection without replacing either endpoint.
    pub(crate) fn invalidate_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Result<LinkCableDataPlaneSnapshot, RoomError> {
        self.data_plane
            .invalidate_connection(connection_id)
            .map_err(Self::map_data_plane_error)
    }

    /// Synchronizes authoritative room/provider epochs without rotating scope.
    pub(crate) fn synchronize_epochs(
        &self,
        room_epoch: u64,
        session_epoch: u64,
    ) -> Result<(), RoomError> {
        self.data_plane
            .synchronize_epochs(room_epoch, session_epoch)
            .map_err(Self::map_data_plane_error)
    }

    /// Permanently closes this room's private data plane.
    pub(crate) fn close(&self) -> Result<(), RoomError> {
        self.data_plane.close().map_err(Self::map_data_plane_error)
    }

    /// Maps private-provider failures onto the stable room-domain vocabulary.
    pub(crate) fn map_data_plane_error(error: LinkCableDataPlaneError) -> RoomError {
        let diagnostic_class = error.diagnostic_class();
        match error {
            LinkCableDataPlaneError::Closed => RoomError::RoomClosed,
            LinkCableDataPlaneError::ConnectionNotAttached
            | LinkCableDataPlaneError::AttachmentReplaced => RoomError::UnknownConnection,
            LinkCableDataPlaneError::RoomEpochMismatch => RoomError::StaleRoomEpoch,
            LinkCableDataPlaneError::SessionEpochMismatch => RoomError::StaleSessionEpoch,
            LinkCableDataPlaneError::NotActive | LinkCableDataPlaneError::TargetUnavailable => {
                RoomError::NotPlaying
            }
            LinkCableDataPlaneError::EndpointAlreadyBound
            | LinkCableDataPlaneError::ReceiverAlreadyClaimed
            | LinkCableDataPlaneError::ConnectionAlreadyBound => RoomError::RoomNotReady,
            LinkCableDataPlaneError::InvalidCapacity
            | LinkCableDataPlaneError::EpochOutOfRange
            | LinkCableDataPlaneError::InvalidPlayerIndex
            | LinkCableDataPlaneError::AuthenticatedSlotMismatch
            | LinkCableDataPlaneError::EnvelopeSequenceMismatch
            | LinkCableDataPlaneError::EnvelopeTimeMismatch
            | LinkCableDataPlaneError::WireIdentityMismatch
            | LinkCableDataPlaneError::SenderSequenceMismatch
            | LinkCableDataPlaneError::InvalidPacketSize
            | LinkCableDataPlaneError::WireCodec(_)
            | LinkCableDataPlaneError::Transaction(_)
            | LinkCableDataPlaneError::QueueOverflow
            | LinkCableDataPlaneError::CableEpochExhausted
            | LinkCableDataPlaneError::AttachmentGenerationExhausted
            | LinkCableDataPlaneError::LifecycleRevisionExhausted
            | LinkCableDataPlaneError::StatePoisoned => {
                RoomError::LinkPacketInvalid { diagnostic_class }
            }
            LinkCableDataPlaneError::EnvelopeSlotMismatch(player_index) => {
                RoomError::SlotSpoofing(player_index)
            }
        }
    }

    /// Stores one player's provisional link compatibility.
    pub(crate) fn set_compatibility(
        &mut self,
        player_index: PlayerIndex,
        protocol_version: u16,
        link: &LinkCableDescriptor,
        compatibility: LinkCableCompatibility,
    ) -> Result<(), RoomError> {
        self.state
            .set_compatibility(player_index, protocol_version, link, compatibility)
    }

    /// Returns whether every connected player supplied link compatibility.
    pub(crate) fn connected_players_have_compatibility(
        &self,
        connected_players: &[PlayerIndex],
        max_players: u8,
    ) -> bool {
        self.state
            .connected_players_have_compatibility(connected_players, max_players)
    }

    /// Returns whether all connected link compatibility values agree.
    pub(crate) fn connected_players_are_compatible(
        &self,
        connected_players: &[PlayerIndex],
        max_players: u8,
    ) -> bool {
        self.state
            .connected_players_are_compatible(connected_players, max_players)
    }
}

#[cfg(test)]
mod tests {
    use super::LinkCableSession;
    use crate::protocol::LinkCableWireCodecError;
    use crate::rooms::{LinkCableDataPlaneError, LinkCableTransactionError, RoomError};

    #[test]
    fn mapping_preserves_static_link_rejection_class() {
        assert_eq!(
            LinkCableSession::map_data_plane_error(LinkCableDataPlaneError::Transaction(
                LinkCableTransactionError::TransferAlreadyPending,
            )),
            RoomError::LinkPacketInvalid {
                diagnostic_class: "transactionTransferAlreadyPending",
            }
        );
        assert_eq!(
            LinkCableSession::map_data_plane_error(LinkCableDataPlaneError::WireCodec(
                LinkCableWireCodecError::UnsupportedMagic,
            )),
            RoomError::LinkPacketInvalid {
                diagnostic_class: "wireUnsupportedMagic",
            }
        );
    }
}
