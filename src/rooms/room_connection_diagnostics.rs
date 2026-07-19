//! Sanitized connection lifecycle descriptions for relay diagnostics.
//!
//! These helpers keep transport-close reporting out of the room mutation code
//! while avoiding raw subject ids, tokens, or input payloads in operator logs.

use crate::rooms::{
    ConnectionId, NetplayRoom, PlayerRole, PlayerRuntimeState, PlayerSlot, PlayerStatus, RoomStatus,
};

impl NetplayRoom {
    /// Describes a control socket connection for debug events.
    pub(super) fn describe_control_connection(
        &self,
        connection_id: ConnectionId,
        reason: &str,
    ) -> String {
        match self
            .players
            .iter()
            .find(|slot| slot.connection_id == Some(connection_id))
        {
            Some(slot) => self.describe_slot("control", slot, reason),
            None => self.describe_unknown_connection("control", reason),
        }
    }

    /// Describes a binary input socket connection for debug events.
    pub(super) fn describe_input_connection(
        &self,
        connection_id: ConnectionId,
        reason: &str,
    ) -> String {
        match self
            .players
            .iter()
            .find(|slot| slot.input_connection_id == Some(connection_id))
        {
            Some(slot) => self.describe_slot("input", slot, reason),
            None => self.describe_unknown_connection("input", reason),
        }
    }

    fn describe_slot(&self, socket: &'static str, slot: &PlayerSlot, reason: &str) -> String {
        format!(
            "{socket} socket closed for p{} role={} client={} roomStatus={} playerStatus={} runtime={} roomEpoch={} sessionEpoch={} reason={}",
            slot.player_index.display_number(),
            role_label(slot.role),
            client_kind_label(slot.subject_key.as_deref()),
            room_status_label(self.status),
            player_status_label(slot.status),
            runtime_state_label(slot.runtime_state),
            self.room_epoch,
            self.session_epoch,
            sanitize_reason(reason),
        )
    }

    fn describe_unknown_connection(&self, socket: &'static str, reason: &str) -> String {
        format!(
            "{socket} socket closed for unknown connection roomStatus={} roomEpoch={} sessionEpoch={} reason={}",
            room_status_label(self.status),
            self.room_epoch,
            self.session_epoch,
            sanitize_reason(reason),
        )
    }
}

fn client_kind_label(subject_key: Option<&str>) -> &str {
    match subject_key.and_then(|key| key.split_once(':').map(|(prefix, _)| prefix)) {
        Some("android") => "android",
        Some("desktop") => "desktop",
        Some(_) => "unknown",
        None => "unknown",
    }
}

fn role_label(role: PlayerRole) -> &'static str {
    match role {
        PlayerRole::Host => "host",
        PlayerRole::Guest => "guest",
    }
}

fn room_status_label(status: RoomStatus) -> &'static str {
    match status {
        RoomStatus::WaitingForGuest => "waitingForGuest",
        RoomStatus::CheckingCompatibility => "checkingCompatibility",
        RoomStatus::SyncingState => "syncingState",
        RoomStatus::Ready => "ready",
        RoomStatus::StartScheduled => "startScheduled",
        RoomStatus::Playing => "playing",
        RoomStatus::Paused => "paused",
        RoomStatus::RepairingState => "repairingState",
        RoomStatus::Recovering => "recovering",
        RoomStatus::Closed => "closed",
    }
}

fn player_status_label(status: PlayerStatus) -> &'static str {
    match status {
        PlayerStatus::Empty => "empty",
        PlayerStatus::Connected => "connected",
        PlayerStatus::CheckingCompatibility => "checkingCompatibility",
        PlayerStatus::CompatibilityFailed => "compatibilityFailed",
        PlayerStatus::SyncingState => "syncingState",
        PlayerStatus::Ready => "ready",
        PlayerStatus::Playing => "playing",
        PlayerStatus::Paused => "paused",
        PlayerStatus::Reconnecting => "reconnecting",
        PlayerStatus::RecoveryExpired => "recoveryExpired",
        PlayerStatus::Disconnected => "disconnected",
    }
}

fn runtime_state_label(state: PlayerRuntimeState) -> &'static str {
    match state {
        PlayerRuntimeState::Empty => "empty",
        PlayerRuntimeState::Connected => "connected",
        PlayerRuntimeState::CheckingCompatibility => "checkingCompatibility",
        PlayerRuntimeState::Syncing => "syncing",
        PlayerRuntimeState::Ready => "ready",
        PlayerRuntimeState::DeterministicReady => "deterministicReady",
        PlayerRuntimeState::Playing => "playing",
        PlayerRuntimeState::Pausing => "pausing",
        PlayerRuntimeState::Paused => "paused",
        PlayerRuntimeState::Reconnecting => "reconnecting",
        PlayerRuntimeState::Stale => "stale",
        PlayerRuntimeState::Disconnected => "disconnected",
        PlayerRuntimeState::RecoveryExpired => "recoveryExpired",
    }
}

fn sanitize_reason(reason: &str) -> String {
    let compact = reason
        .replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if compact.is_empty() {
        return "unspecified".to_string();
    }

    compact.chars().take(160).collect()
}

#[cfg(test)]
mod tests {
    use super::sanitize_reason;

    #[test]
    fn sanitizes_multiline_close_reason() {
        assert_eq!(
            sanitize_reason("runtime\nfailed\tbad packet"),
            "runtime failed bad packet"
        );
    }
}
