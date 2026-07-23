//! Mutually exclusive gameplay providers for one room.
//!
//! Wire descriptors still identify the requested mode. This enum is the
//! server-side ownership boundary that guarantees each room constructs exactly
//! one gameplay provider.

use crate::protocol::{NetplaySessionDescriptor, NetplaySessionMode};
use crate::rooms::{ControllerNetplaySession, LinkCableSession};

/// The one gameplay provider owned by a room.
#[derive(Clone, Debug)]
pub(crate) enum GameplaySession {
    /// Deterministic controller-input netplay.
    ControllerNetplay(ControllerNetplaySession),
    /// Real handheld link-cable transport.
    LinkCable(LinkCableSession),
}

impl GameplaySession {
    /// Selects exactly one provider from the validated wire descriptor.
    pub(crate) fn new(descriptor: &NetplaySessionDescriptor) -> Self {
        match descriptor.mode {
            NetplaySessionMode::ControllerNetplay => {
                Self::ControllerNetplay(ControllerNetplaySession)
            }
            NetplaySessionMode::LinkCable => Self::LinkCable(LinkCableSession::default()),
        }
    }

    /// Returns whether this room owns the controller-netplay provider.
    pub(crate) fn is_controller_netplay(&self) -> bool {
        self.controller_netplay().is_some()
    }

    /// Returns whether this room owns the link-cable provider.
    pub(crate) fn is_link_cable(&self) -> bool {
        matches!(self, Self::LinkCable(_))
    }

    /// Returns the controller provider when this is a controller room.
    pub(crate) fn controller_netplay(&self) -> Option<&ControllerNetplaySession> {
        match self {
            Self::ControllerNetplay(session) => Some(session),
            Self::LinkCable(_) => None,
        }
    }

    /// Returns the link provider when this is a link-cable room.
    pub(crate) fn link_cable(&self) -> Option<&LinkCableSession> {
        match self {
            Self::LinkCable(session) => Some(session),
            Self::ControllerNetplay(_) => None,
        }
    }

    /// Returns the mutable link provider when this is a link-cable room.
    pub(crate) fn link_cable_mut(&mut self) -> Option<&mut LinkCableSession> {
        match self {
            Self::LinkCable(session) => Some(session),
            Self::ControllerNetplay(_) => None,
        }
    }

    /// Clears provider state before a new compatibility cycle.
    pub(crate) fn reset(&mut self) {
        if let Self::LinkCable(session) = self {
            session.reset();
        }
    }
}
