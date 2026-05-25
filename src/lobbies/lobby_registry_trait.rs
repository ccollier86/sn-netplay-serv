//! Lobby registry interface.
//!
//! Routes and transports depend on this trait instead of the in-memory lobby
//! implementation.

use crate::auth::VerifiedLicense;
use crate::lobbies::{CreateLobbyParams, JoinLobbyParams, LobbyError, LobbyJoin, LobbyView};
use crate::rooms::InviteCode;

/// Lobby storage behavior used by HTTP and future WebSocket transports.
#[async_trait::async_trait]
pub trait LobbyRegistry: Send + Sync {
    /// Creates a lobby and reserves Player 1 for the host.
    async fn create_lobby(
        &self,
        host: VerifiedLicense,
        params: CreateLobbyParams,
    ) -> Result<LobbyJoin, LobbyError>;

    /// Adds or refreshes a player in an existing lobby.
    async fn join_lobby(
        &self,
        invite_code: InviteCode,
        player: VerifiedLicense,
        params: JoinLobbyParams,
    ) -> Result<LobbyJoin, LobbyError>;

    /// Returns the current lobby view.
    async fn lobby_view(&self, invite_code: InviteCode) -> Result<LobbyView, LobbyError>;
}
