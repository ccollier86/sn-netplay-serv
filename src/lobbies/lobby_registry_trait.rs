//! Lobby registry interface.
//!
//! Routes and transports depend on this trait instead of the in-memory lobby
//! implementation.

use crate::auth::VerifiedLicense;
use crate::lobbies::{
    CreateLobbyParams, JoinLobbyParams, LobbyChatMessageView, LobbyError, LobbyEvent,
    LobbyGameCandidate, LobbyGameReadinessStatus, LobbyJoin, LobbyRomRelayLimits,
    LobbyRomRelayTransferIntent, LobbyView,
};
use crate::protocol::LobbyFileRelayGrantPair;
use crate::rooms::{ConnectionId, InviteCode, PlayerIndex};
use tokio::sync::broadcast;

/// Receiver for lobby domain events.
pub type LobbyEventReceiver = broadcast::Receiver<LobbyEvent>;

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

    /// Connects a lobby WebSocket for a verified player.
    async fn connect_lobby(
        &self,
        invite_code: InviteCode,
        player: VerifiedLicense,
        params: JoinLobbyParams,
        connection_id: ConnectionId,
    ) -> Result<LobbyJoin, LobbyError>;

    /// Reclaims a lobby slot with a valid resume token.
    async fn reconnect_lobby_player(
        &self,
        invite_code: InviteCode,
        player: VerifiedLicense,
        params: JoinLobbyParams,
        player_index: PlayerIndex,
        lobby_epoch: u64,
        resume_token: String,
        connection_id: ConnectionId,
    ) -> Result<LobbyJoin, LobbyError>;

    /// Marks a lobby WebSocket disconnected.
    async fn disconnect_lobby(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<LobbyView, LobbyError>;

    /// Ends lobby membership for an intentional leave.
    async fn leave_lobby(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<LobbyView, LobbyError>;

    /// Subscribes to domain events for one active lobby.
    async fn subscribe_lobby(
        &self,
        invite_code: InviteCode,
    ) -> Result<LobbyEventReceiver, LobbyError>;

    /// Host selects or replaces the proposed lobby game.
    async fn select_lobby_game(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        game: LobbyGameCandidate,
    ) -> Result<LobbyView, LobbyError>;

    /// Records local readiness for the selected game proposal.
    async fn set_lobby_game_readiness(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        status: LobbyGameReadinessStatus,
        detail: Option<String>,
    ) -> Result<LobbyView, LobbyError>;

    /// Requests launch after all connected players are ready.
    async fn request_lobby_game_launch(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
    ) -> Result<LobbyView, LobbyError>;

    /// Validates a host-to-player temporary ROM transfer request.
    async fn prepare_lobby_rom_relay_transfer(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        receiver_player_index: PlayerIndex,
        limits: LobbyRomRelayLimits,
    ) -> Result<LobbyRomRelayTransferIntent, LobbyError>;

    /// Sends private ROM transfer grants to the involved lobby sockets.
    async fn grant_lobby_rom_relay_transfer(
        &self,
        invite_code: InviteCode,
        intent: LobbyRomRelayTransferIntent,
        grants: LobbyFileRelayGrantPair,
    ) -> Result<(), LobbyError>;

    /// Publishes the direct gameplay room invite created by the host.
    async fn publish_lobby_game_room(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        room_invite_code: InviteCode,
    ) -> Result<LobbyView, LobbyError>;

    /// Clears the active child game and returns players to lobby setup.
    async fn return_lobby_from_game(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
    ) -> Result<LobbyView, LobbyError>;

    /// Sends a sanitized lobby chat message.
    async fn send_lobby_chat(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        body: String,
    ) -> Result<LobbyChatMessageView, LobbyError>;

    /// Returns the current lobby view.
    async fn lobby_view(&self, invite_code: InviteCode) -> Result<LobbyView, LobbyError>;
}
