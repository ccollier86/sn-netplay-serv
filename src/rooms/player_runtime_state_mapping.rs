//! Runtime-state conversion rules shared by room connection operations.

use crate::protocol::ClientRuntimeState;
use crate::rooms::PlayerRuntimeState;

impl From<ClientRuntimeState> for PlayerRuntimeState {
    fn from(state: ClientRuntimeState) -> Self {
        match state {
            ClientRuntimeState::Connected => PlayerRuntimeState::Connected,
            ClientRuntimeState::CheckingCompatibility => PlayerRuntimeState::CheckingCompatibility,
            ClientRuntimeState::Syncing => PlayerRuntimeState::Syncing,
            ClientRuntimeState::Ready => PlayerRuntimeState::Ready,
            ClientRuntimeState::DeterministicReady => PlayerRuntimeState::DeterministicReady,
            ClientRuntimeState::Playing => PlayerRuntimeState::Playing,
            ClientRuntimeState::Pausing => PlayerRuntimeState::Pausing,
            ClientRuntimeState::Paused => PlayerRuntimeState::Paused,
            ClientRuntimeState::Reconnecting => PlayerRuntimeState::Reconnecting,
            ClientRuntimeState::Disconnected => PlayerRuntimeState::Disconnected,
        }
    }
}
