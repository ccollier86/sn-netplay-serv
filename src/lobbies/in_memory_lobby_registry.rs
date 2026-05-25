//! In-memory lobby registry.
//!
//! This registry owns lobby invite-code lookup and slot mutation locking. It is
//! intentionally separate from the active game-room registry.

use crate::auth::VerifiedLicense;
use crate::lobbies::{
    CreateLobbyParams, JoinLobbyParams, Lobby, LobbyError, LobbyJoin, LobbyRegistry,
    LobbyServerCapabilities, LobbyView, MAX_LOBBY_PLAYERS,
};
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, ResumeTokenGenerator, UuidResumeTokenGenerator,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Thread-safe in-memory lobby registry.
pub struct InMemoryLobbyRegistry {
    lobbies: RwLock<HashMap<String, Lobby>>,
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
        let lobby_view = lobby.view(self.capabilities.clone());

        self.lobbies
            .write()
            .await
            .insert(invite_code.normalized().to_string(), lobby);

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
        let player_index = lobby.join_or_refresh_player(
            &player,
            ConnectionId::new(),
            params.display_name,
            params.capabilities,
            resume_token.hash(),
            crate::rooms::current_timestamp_ms(),
        )?;

        Ok(LobbyJoin {
            lobby: lobby.view(self.capabilities.clone()),
            player_index,
            resume_token: resume_token.expose().to_string(),
        })
    }

    async fn lobby_view(&self, invite_code: InviteCode) -> Result<LobbyView, LobbyError> {
        self.lobbies
            .read()
            .await
            .get(invite_code.normalized())
            .map(|lobby| lobby.view(self.capabilities.clone()))
            .ok_or(LobbyError::NotFound)
    }
}
