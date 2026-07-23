//! Controller-netplay gameplay provider.
//!
//! Controller-specific runtime state remains on `NetplayRoom` while the
//! provider boundary is introduced. Moving that state behind this type can be
//! done incrementally without allowing a room to own both gameplay providers.

/// Marker for the controller-netplay provider selected by a room.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ControllerNetplaySession;
