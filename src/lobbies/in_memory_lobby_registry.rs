//! In-memory lobby registry.
//!
//! This registry owns lobby invite-code lookup and slot mutation locking. It is
//! intentionally separate from the active game-room registry.

use crate::auth::VerifiedLicense;
use crate::lobbies::{
    CreateLobbyParams, JoinLobbyParams, Lobby, LobbyChatMessageView, LobbyError,
    LobbyEventReceiver, LobbyGameCandidate, LobbyJoin, LobbyRegistry, LobbyServerCapabilities,
    LobbyView, MAX_LOBBY_PLAYERS, StoredLobby,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex, ResumeTokenGenerator,
    UuidResumeTokenGenerator,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Thread-safe in-memory lobby registry.
pub struct InMemoryLobbyRegistry {
    lobbies: RwLock<HashMap<String, StoredLobby>>,
    invite_code_generator: Arc<dyn InviteCodeGenerator>,
    resume_token_generator: Arc<dyn ResumeTokenGenerator>,
    capabilities: LobbyServerCapabilities,
}

impl InMemoryLobbyRegistry {
    /// Creates an empty lobby registry.
    pub fn new(invite_code_generator: Arc<dyn InviteCodeGenerator>) -> Self {
        Self {
            lobbies: RwLock::new(HashMap::new()),
            invite_code_generator,
            resume_token_generator: Arc::new(UuidResumeTokenGenerator),
            capabilities: LobbyServerCapabilities::current(MAX_LOBBY_PLAYERS, false, false),
        }
    }

    /// Creates a registry with injectable generators for tests.
    pub fn with_generators(
        invite_code_generator: Arc<dyn InviteCodeGenerator>,
        resume_token_generator: Arc<dyn ResumeTokenGenerator>,
    ) -> Self {
        Self {
            lobbies: RwLock::new(HashMap::new()),
            invite_code_generator,
            resume_token_generator,
            capabilities: LobbyServerCapabilities::current(MAX_LOBBY_PLAYERS, false, false),
        }
    }

    /// Creates a registry with injectable generators and server capabilities.
    pub fn with_generators_and_capabilities(
        invite_code_generator: Arc<dyn InviteCodeGenerator>,
        resume_token_generator: Arc<dyn ResumeTokenGenerator>,
        capabilities: LobbyServerCapabilities,
    ) -> Self {
        Self {
            lobbies: RwLock::new(HashMap::new()),
            invite_code_generator,
            resume_token_generator,
            capabilities,
        }
    }
}

#[async_trait::async_trait]
impl LobbyRegistry for InMemoryLobbyRegistry {
    async fn create_lobby(
        &self,
        host: VerifiedLicense,
        params: CreateLobbyParams,
    ) -> Result<LobbyJoin, LobbyError> {
        let invite_code = self.invite_code_generator.generate();
        let resume_token = self.resume_token_generator.generate();
        let lobby = Lobby::new(
            invite_code.clone(),
            &host,
            ConnectionId::new(),
            params.display_name,
            params.capabilities,
            resume_token.hash(),
            params.initial_game,
            crate::rooms::current_timestamp_ms(),
        );
        let stored_lobby = StoredLobby::new(lobby, self.capabilities.clone());
        let lobby_view = stored_lobby.view();

        self.lobbies
            .write()
            .await
            .insert(invite_code.normalized().to_string(), stored_lobby);

        Ok(LobbyJoin {
            lobby: lobby_view,
            player_index: crate::rooms::PlayerIndex::ONE,
            resume_token: resume_token.expose().to_string(),
        })
    }

    async fn join_lobby(
        &self,
        invite_code: InviteCode,
        player: VerifiedLicense,
        params: JoinLobbyParams,
    ) -> Result<LobbyJoin, LobbyError> {
        let resume_token = self.resume_token_generator.generate();
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        let player_index = lobby.lobby.join_or_refresh_player(
            &player,
            ConnectionId::new(),
            params.display_name,
            params.capabilities,
            resume_token.hash(),
            crate::rooms::current_timestamp_ms(),
        )?;
        lobby.emit_state_changed();

        Ok(LobbyJoin {
            lobby: lobby.view(),
            player_index,
            resume_token: resume_token.expose().to_string(),
        })
    }

    async fn connect_lobby(
        &self,
        invite_code: InviteCode,
        player: VerifiedLicense,
        params: JoinLobbyParams,
        connection_id: ConnectionId,
    ) -> Result<LobbyJoin, LobbyError> {
        let resume_token = self.resume_token_generator.generate();
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        let player_index = lobby.lobby.join_or_refresh_player(
            &player,
            connection_id,
            params.display_name,
            params.capabilities,
            resume_token.hash(),
            crate::rooms::current_timestamp_ms(),
        )?;
        lobby.emit_state_changed();

        Ok(LobbyJoin {
            lobby: lobby.view(),
            player_index,
            resume_token: resume_token.expose().to_string(),
        })
    }

    async fn reconnect_lobby_player(
        &self,
        invite_code: InviteCode,
        player_index: PlayerIndex,
        lobby_epoch: u64,
        resume_token: String,
        connection_id: ConnectionId,
    ) -> Result<LobbyJoin, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        let player_index = lobby.lobby.reconnect_player(
            player_index,
            lobby_epoch,
            &resume_token,
            connection_id,
            crate::rooms::current_timestamp_ms(),
        )?;
        lobby.emit_state_changed();

        Ok(LobbyJoin {
            lobby: lobby.view(),
            player_index,
            resume_token,
        })
    }

    async fn disconnect_lobby(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<LobbyView, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;

        if lobby
            .lobby
            .disconnect(connection_id, crate::rooms::current_timestamp_ms())
        {
            lobby.emit_state_changed();
        }

        Ok(lobby.view())
    }

    async fn subscribe_lobby(
        &self,
        invite_code: InviteCode,
    ) -> Result<LobbyEventReceiver, LobbyError> {
        self.lobbies
            .read()
            .await
            .get(invite_code.normalized())
            .map(StoredLobby::subscribe)
            .ok_or(LobbyError::NotFound)
    }

    async fn select_lobby_game(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        game: LobbyGameCandidate,
    ) -> Result<LobbyView, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        lobby
            .lobby
            .select_game(connection_id, game, crate::rooms::current_timestamp_ms())?;
        lobby.emit_state_changed();

        Ok(lobby.view())
    }

    async fn send_lobby_chat(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        body: String,
    ) -> Result<LobbyChatMessageView, LobbyError> {
        let lobbies = self.lobbies.read().await;
        let lobby = lobbies
            .get(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        let player_index = lobby.lobby.player_index_for_connection(connection_id)?;
        let chat = LobbyChatMessageView::new(
            player_index,
            sanitize_chat_body(body)?,
            crate::rooms::current_timestamp_ms(),
        );
        lobby.emit_chat_message(chat.clone());

        Ok(chat)
    }

    async fn lobby_view(&self, invite_code: InviteCode) -> Result<LobbyView, LobbyError> {
        self.lobbies
            .read()
            .await
            .get(invite_code.normalized())
            .map(StoredLobby::view)
            .ok_or(LobbyError::NotFound)
    }
}

fn sanitize_chat_body(body: String) -> Result<String, LobbyError> {
    let sanitized = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let sanitized = sanitized.trim();

    if sanitized.is_empty() || sanitized.chars().count() > 500 {
        return Err(LobbyError::InvalidPayload);
    }

    Ok(sanitized.to_string())
}
