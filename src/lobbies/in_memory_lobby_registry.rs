//! In-memory lobby registry.
//!
//! This registry owns lobby invite-code lookup and slot mutation locking. It is
//! intentionally separate from the active game-room registry.

use crate::auth::VerifiedLicense;
use crate::lobbies::{
    CreateLobbyParams, JoinLobbyParams, Lobby, LobbyChatMessageView, LobbyDebugEvent,
    LobbyDebugEventLog, LobbyDebugEventSink, LobbyError, LobbyEventReceiver, LobbyGameCandidate,
    LobbyGameReadinessStatus, LobbyJoin, LobbyRegistry, LobbyRegistrySnapshot, LobbyRomRelayLimits,
    LobbyRomRelayTransferIntent, LobbyServerCapabilities, LobbyView, MAX_LOBBY_PLAYERS,
    NoopLobbyDebugEventSink, StoredLobby,
};
use crate::protocol::LobbyFileRelayGrantPair;
use crate::rooms::{
    ConnectionId, InviteCode, InviteCodeGenerator, PlayerIndex, ResumeTokenGenerator,
    UuidResumeTokenGenerator,
};
use crate::voice::{DisabledVoiceBroker, VoiceBroker};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

/// Thread-safe in-memory lobby registry.
pub struct InMemoryLobbyRegistry {
    pub(super) lobbies: RwLock<HashMap<String, StoredLobby>>,
    invite_code_generator: Arc<dyn InviteCodeGenerator>,
    resume_token_generator: Arc<dyn ResumeTokenGenerator>,
    pub(super) capabilities: LobbyServerCapabilities,
    recent_events: Mutex<LobbyDebugEventLog>,
    event_sink: Arc<dyn LobbyDebugEventSink>,
    pub(super) voice_broker: Arc<dyn VoiceBroker>,
}

impl InMemoryLobbyRegistry {
    /// Creates an empty lobby registry.
    pub fn new(invite_code_generator: Arc<dyn InviteCodeGenerator>) -> Self {
        Self {
            lobbies: RwLock::new(HashMap::new()),
            invite_code_generator,
            resume_token_generator: Arc::new(UuidResumeTokenGenerator),
            capabilities: LobbyServerCapabilities::current(MAX_LOBBY_PLAYERS, false, false),
            recent_events: Mutex::new(LobbyDebugEventLog::default()),
            event_sink: Arc::new(NoopLobbyDebugEventSink),
            voice_broker: Arc::new(DisabledVoiceBroker),
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
            recent_events: Mutex::new(LobbyDebugEventLog::default()),
            event_sink: Arc::new(NoopLobbyDebugEventSink),
            voice_broker: Arc::new(DisabledVoiceBroker),
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
            recent_events: Mutex::new(LobbyDebugEventLog::default()),
            event_sink: Arc::new(NoopLobbyDebugEventSink),
            voice_broker: Arc::new(DisabledVoiceBroker),
        }
    }

    /// Creates a registry with injectable generators, capabilities, and voice broker.
    pub fn with_generators_capabilities_and_voice(
        invite_code_generator: Arc<dyn InviteCodeGenerator>,
        resume_token_generator: Arc<dyn ResumeTokenGenerator>,
        capabilities: LobbyServerCapabilities,
        voice_broker: Arc<dyn VoiceBroker>,
    ) -> Self {
        Self::with_generators_capabilities_voice_and_event_sink(
            invite_code_generator,
            resume_token_generator,
            capabilities,
            voice_broker,
            Arc::new(NoopLobbyDebugEventSink),
        )
    }

    /// Creates a registry with injectable generators, capabilities, voice broker, and event sink.
    pub fn with_generators_capabilities_voice_and_event_sink(
        invite_code_generator: Arc<dyn InviteCodeGenerator>,
        resume_token_generator: Arc<dyn ResumeTokenGenerator>,
        capabilities: LobbyServerCapabilities,
        voice_broker: Arc<dyn VoiceBroker>,
        event_sink: Arc<dyn LobbyDebugEventSink>,
    ) -> Self {
        Self {
            lobbies: RwLock::new(HashMap::new()),
            invite_code_generator,
            resume_token_generator,
            capabilities,
            recent_events: Mutex::new(LobbyDebugEventLog::default()),
            event_sink,
            voice_broker,
        }
    }

    fn record_lobby_event(&self, lobby: &mut StoredLobby, kind: &str, detail: String) {
        let event = lobby.record_debug_event(kind, detail);
        self.record_recent_event(event);
    }

    fn record_recent_event(&self, event: LobbyDebugEvent) {
        self.event_sink.record_lobby_event(event.clone());

        let Ok(mut recent_events) = self.recent_events.lock() else {
            return;
        };

        recent_events.push(event);
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
        let mut lobby = Lobby::new(
            invite_code.clone(),
            &host,
            ConnectionId::new(),
            params.display_name.clone(),
            params.capabilities.clone(),
            resume_token.hash(),
            params.initial_game,
            crate::rooms::current_timestamp_ms(),
        );
        if let Some(voice) = self
            .create_voice_state_for_lobby(
                &lobby,
                params.voice.as_ref(),
                params.capabilities.supports_lobby_voice,
            )
            .await
        {
            lobby.set_voice_state(voice);
        }
        let voice = lobby.voice_grant_for(crate::rooms::PlayerIndex::ONE);
        let mut stored_lobby = StoredLobby::new(lobby, self.capabilities.clone());
        let created_detail = lobby_created_detail(&stored_lobby.view());
        self.record_lobby_event(&mut stored_lobby, "lobbyCreated", created_detail);
        let lobby_view = stored_lobby.view();

        self.lobbies
            .write()
            .await
            .insert(invite_code.normalized().to_string(), stored_lobby);

        Ok(LobbyJoin {
            lobby: lobby_view,
            player_index: crate::rooms::PlayerIndex::ONE,
            resume_token: resume_token.expose().to_string(),
            voice,
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
        self.record_lobby_event(
            lobby,
            "lobbyJoined",
            player_detail("player joined lobby", player_index),
        );

        Ok(LobbyJoin {
            lobby: lobby.view(),
            player_index,
            resume_token: resume_token.expose().to_string(),
            voice: lobby.lobby.voice_grant_for(player_index),
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
        self.record_lobby_event(
            lobby,
            "lobbySocketConnected",
            player_detail("lobby socket connected", player_index),
        );

        Ok(LobbyJoin {
            lobby: lobby.view(),
            player_index,
            resume_token: resume_token.expose().to_string(),
            voice: lobby.lobby.voice_grant_for(player_index),
        })
    }

    async fn reconnect_lobby_player(
        &self,
        invite_code: InviteCode,
        player: VerifiedLicense,
        params: JoinLobbyParams,
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
            &player,
            player_index,
            lobby_epoch,
            &resume_token,
            connection_id,
            params.display_name,
            params.capabilities,
            crate::rooms::current_timestamp_ms(),
        )?;
        lobby.emit_state_changed();
        self.record_lobby_event(
            lobby,
            "lobbyPlayerReconnected",
            player_detail("lobby player reconnected", player_index),
        );

        Ok(LobbyJoin {
            lobby: lobby.view(),
            player_index,
            resume_token,
            voice: lobby.lobby.voice_grant_for(player_index),
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
            let detail = "lobby socket disconnected".to_string();
            self.record_lobby_event(lobby, "lobbySocketDisconnected", detail);
        }

        Ok(lobby.view())
    }

    async fn leave_lobby(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<LobbyView, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;

        lobby
            .lobby
            .leave(connection_id, crate::rooms::current_timestamp_ms())?;
        let voice_cleanup = if lobby.lobby.status() == crate::lobbies::LobbyStatus::Closed {
            lobby.lobby.take_voice_room_id_for_cleanup()
        } else {
            None
        };
        lobby.emit_state_changed();
        let detail = if lobby.lobby.status() == crate::lobbies::LobbyStatus::Closed {
            "host left; lobby closed".to_string()
        } else {
            "player left lobby".to_string()
        };
        self.record_lobby_event(lobby, "lobbyPlayerLeft", detail);
        self.cleanup_lobby_voice_room(voice_cleanup, "lobby-left");

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
        self.record_lobby_event(lobby, "lobbyGameSelected", "host selected game".to_string());

        Ok(lobby.view())
    }

    async fn set_lobby_game_readiness(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        status: LobbyGameReadinessStatus,
        detail: Option<String>,
    ) -> Result<LobbyView, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        lobby.lobby.set_game_readiness(
            connection_id,
            proposal_id,
            status,
            detail,
            crate::rooms::current_timestamp_ms(),
        )?;
        lobby.emit_state_changed();
        self.record_lobby_event(
            lobby,
            "lobbyReadinessSet",
            format!("player readiness set status={status:?}"),
        );

        Ok(lobby.view())
    }

    async fn request_lobby_game_launch(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
    ) -> Result<LobbyView, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        lobby.lobby.request_game_launch(
            connection_id,
            proposal_id,
            crate::rooms::current_timestamp_ms(),
        )?;
        lobby.emit_state_changed();
        self.record_lobby_event(
            lobby,
            "lobbyLaunchRequested",
            "host requested game launch".to_string(),
        );

        Ok(lobby.view())
    }

    async fn prepare_lobby_rom_relay_transfer(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        receiver_player_index: PlayerIndex,
        limits: LobbyRomRelayLimits,
    ) -> Result<LobbyRomRelayTransferIntent, LobbyError> {
        let lobbies = self.lobbies.read().await;
        let lobby = lobbies
            .get(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;

        lobby.lobby.prepare_rom_relay_transfer(
            connection_id,
            proposal_id,
            receiver_player_index,
            limits,
        )
    }

    async fn grant_lobby_rom_relay_transfer(
        &self,
        invite_code: InviteCode,
        intent: LobbyRomRelayTransferIntent,
        grants: LobbyFileRelayGrantPair,
    ) -> Result<(), LobbyError> {
        let lobbies = self.lobbies.read().await;
        let lobby = lobbies
            .get(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;

        lobby.lobby.require_rom_relay_transfer_current(&intent)?;
        lobby.emit_rom_transfer_grants(
            intent.sender_connection_id,
            intent.receiver_connection_id,
            grants,
        );

        Ok(())
    }

    async fn publish_lobby_game_room(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
        room_invite_code: InviteCode,
    ) -> Result<LobbyView, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        lobby.lobby.publish_game_room(
            connection_id,
            proposal_id,
            room_invite_code,
            crate::rooms::current_timestamp_ms(),
        )?;
        lobby.emit_state_changed();
        self.record_lobby_event(
            lobby,
            "lobbyGameRoomPublished",
            "gameplay room published to lobby".to_string(),
        );

        Ok(lobby.view())
    }

    async fn return_lobby_from_game(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        proposal_id: uuid::Uuid,
    ) -> Result<LobbyView, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        lobby.lobby.return_to_lobby(
            connection_id,
            proposal_id,
            crate::rooms::current_timestamp_ms(),
        )?;
        lobby.emit_state_changed();
        self.record_lobby_event(
            lobby,
            "lobbyReturned",
            "lobby returned from game".to_string(),
        );

        Ok(lobby.view())
    }

    async fn send_lobby_chat(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        body: String,
    ) -> Result<LobbyChatMessageView, LobbyError> {
        let mut lobbies = self.lobbies.write().await;
        let lobby = lobbies
            .get_mut(invite_code.normalized())
            .ok_or(LobbyError::NotFound)?;
        let player_index = lobby.lobby.player_index_for_connection(connection_id)?;
        let chat = LobbyChatMessageView::new(
            player_index,
            sanitize_chat_body(body)?,
            crate::rooms::current_timestamp_ms(),
        );
        lobby.emit_chat_message(chat.clone());
        self.record_lobby_event(
            lobby,
            "lobbyChatMessage",
            player_detail("lobby chat message sent", player_index),
        );

        Ok(chat)
    }

    async fn refresh_lobby_voice_token(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
    ) -> Result<crate::lobbies::LobbyVoiceTokenRefresh, LobbyError> {
        let refresh = self
            .refresh_lobby_voice_token_impl(invite_code.clone(), connection_id)
            .await?;
        let mut lobbies = self.lobbies.write().await;

        if let Some(lobby) = lobbies.get_mut(invite_code.normalized()) {
            self.record_lobby_event(
                lobby,
                "lobbyVoiceTokenRefreshed",
                "lobby voice token refreshed".to_string(),
            );
        }

        Ok(refresh)
    }

    async fn lobby_view(&self, invite_code: InviteCode) -> Result<LobbyView, LobbyError> {
        self.lobbies
            .read()
            .await
            .get(invite_code.normalized())
            .map(StoredLobby::view)
            .ok_or(LobbyError::NotFound)
    }

    async fn snapshot(&self) -> LobbyRegistrySnapshot {
        let mut lobbies = self
            .lobbies
            .read()
            .await
            .values()
            .map(StoredLobby::view)
            .collect::<Vec<_>>();

        lobbies.sort_by(|left, right| left.created_at_ms.cmp(&right.created_at_ms));

        LobbyRegistrySnapshot {
            active_lobby_count: lobbies.len(),
            lobbies,
        }
    }

    async fn lobby_events(
        &self,
        invite_code: InviteCode,
        limit: usize,
    ) -> Result<Vec<LobbyDebugEvent>, LobbyError> {
        self.lobbies
            .read()
            .await
            .get(invite_code.normalized())
            .map(|lobby| lobby.debug_events(limit))
            .ok_or(LobbyError::NotFound)
    }

    async fn recent_events(&self, limit: usize) -> Vec<LobbyDebugEvent> {
        let Ok(events) = self.recent_events.lock() else {
            return Vec::new();
        };

        events.tail(limit)
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

fn lobby_created_detail(lobby: &LobbyView) -> String {
    format!(
        "lobby created status={:?} voice={} selectedGame={}",
        lobby.status,
        lobby_voice_state(lobby),
        lobby.selected_game.is_some()
    )
}

fn lobby_voice_state(lobby: &LobbyView) -> &'static str {
    match lobby.voice.as_ref().map(|voice| voice.status) {
        Some(crate::rooms::RoomVoiceStatus::Available) => "available",
        Some(crate::rooms::RoomVoiceStatus::Unavailable) => "unavailable",
        None => "off",
    }
}

fn player_detail(prefix: &str, player_index: PlayerIndex) -> String {
    format!("{prefix} p{}", player_index.zero_based() + 1)
}
