//! In-memory registry for active netplay rooms.
//!
//! The registry owns invite-code lookup and room mutation synchronization. It
//! does not validate licenses or serialize HTTP responses directly.

use super::stored_room::StoredRoom;
use crate::protocol::SessionPauseReason;
#[cfg(test)]
use crate::rooms::RoomRegistry;
use crate::rooms::{
    Clock, ConnectionId, InviteCode, InviteCodeGenerator, NoopRoomDebugEventSink,
    ResumeTokenGenerator, RoomDebugEvent, RoomDebugEventLog, RoomDebugEventSink, RoomError,
    RoomPerformanceSample, RoomRecoveryConfig, RoomView, SystemClock, UuidResumeTokenGenerator,
};
use crate::voice::{DisabledVoiceBroker, VoiceBroker};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[path = "room_registry_lifecycle_ops.rs"]
mod lifecycle_ops;
#[path = "room_registry_query_ops.rs"]
mod query_ops;
#[path = "room_registry_relay_ops.rs"]
mod relay_ops;
#[path = "room_registry_sync_ops.rs"]
mod sync_ops;
#[path = "room_registry_trait_impl.rs"]
mod trait_impl;
#[path = "room_registry_voice_ops.rs"]
mod voice_ops;

/// Thread-safe in-memory room registry.
pub struct InMemoryRoomRegistry {
    invite_codes: RwLock<HashMap<String, StoredRoom>>,
    invite_code_generator: Arc<dyn InviteCodeGenerator>,
    resume_token_generator: Arc<dyn ResumeTokenGenerator>,
    clock: Arc<dyn Clock>,
    recovery_config: RoomRecoveryConfig,
    recent_events: Mutex<RoomDebugEventLog>,
    event_sink: Arc<dyn RoomDebugEventSink>,
    voice_broker: Arc<dyn VoiceBroker>,
}

impl InMemoryRoomRegistry {
    /// Creates an empty registry with the supplied invite-code generator.
    pub fn new(invite_code_generator: Arc<dyn InviteCodeGenerator>) -> Self {
        Self::with_dependencies(
            invite_code_generator,
            Arc::new(UuidResumeTokenGenerator),
            Arc::new(SystemClock),
            RoomRecoveryConfig::default(),
        )
    }

    /// Creates an empty registry with injectable lifecycle dependencies.
    pub fn with_dependencies(
        invite_code_generator: Arc<dyn InviteCodeGenerator>,
        resume_token_generator: Arc<dyn ResumeTokenGenerator>,
        clock: Arc<dyn Clock>,
        recovery_config: RoomRecoveryConfig,
    ) -> Self {
        Self::with_dependencies_and_event_sink(
            invite_code_generator,
            resume_token_generator,
            clock,
            recovery_config,
            Arc::new(NoopRoomDebugEventSink),
        )
    }

    /// Creates a registry with an external nonblocking event sink.
    pub fn with_dependencies_and_event_sink(
        invite_code_generator: Arc<dyn InviteCodeGenerator>,
        resume_token_generator: Arc<dyn ResumeTokenGenerator>,
        clock: Arc<dyn Clock>,
        recovery_config: RoomRecoveryConfig,
        event_sink: Arc<dyn RoomDebugEventSink>,
    ) -> Self {
        Self::with_dependencies_event_sink_and_voice(
            invite_code_generator,
            resume_token_generator,
            clock,
            recovery_config,
            event_sink,
            Arc::new(DisabledVoiceBroker),
        )
    }

    /// Creates a registry with event and voice broker dependencies.
    pub fn with_dependencies_event_sink_and_voice(
        invite_code_generator: Arc<dyn InviteCodeGenerator>,
        resume_token_generator: Arc<dyn ResumeTokenGenerator>,
        clock: Arc<dyn Clock>,
        recovery_config: RoomRecoveryConfig,
        event_sink: Arc<dyn RoomDebugEventSink>,
        voice_broker: Arc<dyn VoiceBroker>,
    ) -> Self {
        Self {
            invite_codes: RwLock::new(HashMap::new()),
            invite_code_generator,
            resume_token_generator,
            clock,
            recovery_config,
            recent_events: Mutex::new(RoomDebugEventLog::default()),
            event_sink,
            voice_broker,
        }
    }

    pub(super) fn record_recent_events(&self, events: Vec<RoomDebugEvent>) {
        let Ok(mut recent_events) = self.recent_events.lock() else {
            return;
        };

        for event in events {
            self.event_sink.record(event.clone());
            recent_events.push(event);
        }
    }

    pub(super) fn record_performance_sample(&self, sample: RoomPerformanceSample) {
        self.event_sink.record_performance_sample(sample);
    }

    /// Removes rooms still waiting for a guest after `join_timeout`.
    pub async fn remove_expired_waiting_rooms(
        &self,
        now: Instant,
        join_timeout: Duration,
    ) -> usize {
        self.sweep_expired_rooms(now, join_timeout).await
    }

    /// Releases one canonical controller frame for each active room.
    pub async fn release_next_controller_frames(&self) -> usize {
        let mut rooms = self.invite_codes.write().await;
        let now = self.clock.now();
        let mut released_count = 0;

        for stored_room in rooms.values_mut() {
            if stored_room.emit_next_server_frame(now).is_some() {
                released_count += 1;
            }
        }

        released_count
    }

    /// Test-facing pause helper that uses an empty idempotency key.
    pub async fn request_session_pause(
        &self,
        invite_code: InviteCode,
        connection_id: ConnectionId,
        reason: SessionPauseReason,
        local_frame: u64,
    ) -> Result<RoomView, RoomError> {
        self.request_session_pause_impl(
            invite_code,
            connection_id,
            String::new(),
            reason,
            local_frame,
        )
        .await
    }
}

#[cfg(test)]
#[path = "room_registry_link_tests.rs"]
mod room_registry_link_tests;

#[cfg(test)]
#[path = "room_registry_tests.rs"]
mod room_registry_tests;

#[cfg(test)]
#[path = "room_registry_voice_tests.rs"]
mod room_registry_voice_tests;
