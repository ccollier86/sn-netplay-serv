package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.RoomView
import app.shadowboy.netplay.sdk.protocol.ServerMessage

public data class NetplayClientState(
    public val room: RoomView? = null,
    public val assignedPlayerIndex: Int? = null,
    public val latestEventSeq: Long = 0,
    public val roomEpoch: Long = 0,
    public val sessionEpoch: Long = 0,
    public val resync: NetplayResyncState? = null,
    public val runtimeResetRequired: Boolean = false,
    public val voice: NetplayVoiceGrantState = NetplayVoiceGrantState(),
    public val lastError: NetplayCloseReason.RelayError? = null,
)

public enum class NetplayEffectivePauseReason {
    User,
    Peer,
    ConnectionRecovery,
    StateResync,
}

public data class NetplayClientDiagnostics(
    public val assignedPlayerIndex: Int?,
    public val effectivePauseReason: NetplayEffectivePauseReason?,
    public val frameClock: FrameClockDiagnostics,
    public val heartbeat: HeartbeatHealth,
    public val heartbeatAck: HeartbeatAckState,
    public val lastError: NetplayCloseReason.RelayError?,
    public val latestEventSeq: Long,
    public val reconnectTicketAvailable: Boolean,
    public val resync: NetplayResyncState?,
    public val roomEpoch: Long,
    public val sessionEpoch: Long,
    public val voice: NetplayVoiceDiagnostics,
)

public class RoomStateMachine(
    public val reconnectTokens: ReconnectTokenStore = ReconnectTokenStore(),
    public val pause: PauseCoordinator = PauseCoordinator(),
    public val heartbeat: HeartbeatTracker = HeartbeatTracker(),
    public val frameClock: FrameClockTracker = FrameClockTracker(),
    public val resync: ResyncCoordinator = ResyncCoordinator(),
    public val voice: NetplayVoiceGrantTracker = NetplayVoiceGrantTracker(),
) {
    public var state: NetplayClientState = NetplayClientState()
        private set

    public fun apply(message: ServerMessage): NetplayClientState {
        if (!isMessageCurrent(message)) {
            return state
        }

        when (message) {
            is ServerMessage.RoomJoined -> {
                reconnectTokens.apply(message)
                voice.applyMessage(message)
                updateRoom(message.room, message.yourPlayerIndex)
            }
            is ServerMessage.RoomStateChanged -> updateRoom(message.room)
            is ServerMessage.CompatibilityRequested -> updateRoom(message.room)
            is ServerMessage.RecoveryStarted -> updateRoom(message.room)
            is ServerMessage.PlayerReconnected -> updateRoom(message.room)
            is ServerMessage.PlayerExited -> updateRoom(message.room)
            is ServerMessage.RecoveryResyncRequired -> {
                resync.apply(message, ResyncContext(assignedPlayerIndex = state.assignedPlayerIndex))
                frameClock.reset()
                updateRoom(message.room)
            }
            is ServerMessage.StateHashMismatch -> {
                resync.apply(message, ResyncContext(assignedPlayerIndex = state.assignedPlayerIndex))
                frameClock.reset()
                updateRoom(message.room)
            }
            is ServerMessage.InputDelayChanged -> updateRoom(message.room)
            is ServerMessage.StartSession -> {
                resync.markComplete()
                resync.clear()
                updateRoom(message.room)
            }
            is ServerMessage.SessionPauseScheduled -> {
                pause.apply(message.pause)
                updateRoom(message.room)
            }
            is ServerMessage.SessionPauseUpdated -> {
                pause.apply(message.pause)
                updateRoom(message.room)
            }
            is ServerMessage.SessionResumeScheduled -> {
                pause.clear(message.sequence)
                updateRoom(message.room)
            }
            is ServerMessage.HeartbeatAck -> updateEpochs(
                eventSeq = message.eventSeq,
                roomEpoch = message.roomEpoch,
                sessionEpoch = message.sessionEpoch,
            )
            is ServerMessage.VoiceTokenRefreshed -> {
                voice.applyMessage(message)
                updateEpochs(
                    eventSeq = message.eventSeq,
                    roomEpoch = message.roomEpoch,
                    sessionEpoch = message.sessionEpoch,
                )
            }
            is ServerMessage.Error -> {
                state = state.copy(lastError = NetplayCloseReason.RelayError(message.code, message.message))
            }
            is ServerMessage.InputFrameMessage -> {
                if (isRuntimeMessageCurrent(message)) {
                    frameClock.markPeerInputFrame(message.input)
                }
            }
            is ServerMessage.ServerFrameMessage -> {
                if (isRuntimeMessageCurrent(message)) {
                    frameClock.applyServerFrame(message.frame)
                }
            }
            ServerMessage.Pong,
            is ServerMessage.LinkCablePacketMessage,
            is ServerMessage.SnapshotChunkMessage,
            is ServerMessage.SnapshotComplete -> Unit
        }

        return state
    }

    public fun acknowledgeRuntimeReset() {
        state = state.copy(runtimeResetRequired = false)
    }

    public fun isMessageCurrent(message: ServerMessage): Boolean {
        val roomEpoch = message.roomEpochOrNull ?: return true
        val sessionEpoch = message.sessionEpochOrNull ?: return true

        if (state.roomEpoch == 0L && state.sessionEpoch == 0L) {
            return true
        }

        if (roomEpoch < state.roomEpoch || sessionEpoch < state.sessionEpoch) {
            return false
        }

        val eventSeq = message.eventSeqOrNull
        return eventSeq == null ||
            roomEpoch != state.roomEpoch ||
            sessionEpoch != state.sessionEpoch ||
            eventSeq >= state.latestEventSeq
    }

    public fun isRuntimeMessageCurrent(message: ServerMessage): Boolean =
        when (message) {
            is ServerMessage.ServerFrameMessage -> isExactRuntimeEpochCurrent(
                message.frame.roomEpoch,
                message.frame.sessionEpoch,
            )
            is ServerMessage.InputFrameMessage -> isExactRuntimeEpochCurrent(
                message.roomEpoch,
                message.sessionEpoch,
            )
            is ServerMessage.SnapshotChunkMessage -> isExactRuntimeEpochCurrent(
                message.roomEpoch,
                message.sessionEpoch,
            )
            is ServerMessage.SnapshotComplete -> isExactRuntimeEpochCurrent(
                message.roomEpoch,
                message.sessionEpoch,
            )
            else -> isMessageCurrent(message)
        }

    public fun effectivePauseReason(): NetplayEffectivePauseReason? {
        val currentResync = resync.currentResync
        if (currentResync != null) {
            return if (currentResync.reason == NetplayResyncReason.Recovery) {
                NetplayEffectivePauseReason.ConnectionRecovery
            } else {
                NetplayEffectivePauseReason.StateResync
            }
        }

        val currentPause = pause.currentPause ?: return null
        return if (currentPause.requestedByPlayerIndex == state.assignedPlayerIndex) {
            NetplayEffectivePauseReason.User
        } else {
            NetplayEffectivePauseReason.Peer
        }
    }

    public fun diagnostics(nowMs: Long): NetplayClientDiagnostics =
        NetplayClientDiagnostics(
            assignedPlayerIndex = state.assignedPlayerIndex,
            effectivePauseReason = effectivePauseReason(),
            frameClock = frameClock.snapshot(),
            heartbeat = heartbeat.health(nowMs),
            heartbeatAck = heartbeat.lastAck(),
            lastError = state.lastError,
            latestEventSeq = state.latestEventSeq,
            reconnectTicketAvailable = reconnectTokens.current() != null,
            resync = resync.currentResync,
            roomEpoch = state.roomEpoch,
            sessionEpoch = state.sessionEpoch,
            voice = voice.diagnostics(),
        )

    public fun reset() {
        reconnectTokens.clear()
        pause.reset()
        frameClock.reset()
        resync.reset()
        voice.reset()
        state = NetplayClientState()
    }

    private fun updateRoom(room: RoomView, assignedPlayerIndex: Int? = state.assignedPlayerIndex) {
        val sessionChanged = state.sessionEpoch != 0L && room.sessionEpoch > state.sessionEpoch
        if (sessionChanged) {
            frameClock.reset()
        }

        reconnectTokens.updateAcceptedEpoch(room.roomEpoch)
        voice.applyRoom(room)
        frameClock.applyRoom(room)
        state = state.copy(
            room = room,
            assignedPlayerIndex = assignedPlayerIndex,
            latestEventSeq = room.eventSeq,
            roomEpoch = room.roomEpoch,
            sessionEpoch = room.sessionEpoch,
            resync = resync.currentResync,
            runtimeResetRequired = state.runtimeResetRequired || sessionChanged,
            voice = voice.state,
            lastError = null,
        )
    }

    private fun updateEpochs(eventSeq: Long, roomEpoch: Long, sessionEpoch: Long) {
        reconnectTokens.updateAcceptedEpoch(roomEpoch)
        state = state.copy(
            latestEventSeq = eventSeq,
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            voice = voice.state,
            runtimeResetRequired = state.runtimeResetRequired ||
                (state.sessionEpoch != 0L && sessionEpoch > state.sessionEpoch),
        )
    }

    private fun isExactRuntimeEpochCurrent(roomEpoch: Long, sessionEpoch: Long): Boolean {
        if (state.roomEpoch == 0L && state.sessionEpoch == 0L) {
            return true
        }

        return roomEpoch == state.roomEpoch && sessionEpoch == state.sessionEpoch
    }
}
