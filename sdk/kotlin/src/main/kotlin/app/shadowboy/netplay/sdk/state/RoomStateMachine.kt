package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.RoomView
import app.shadowboy.netplay.sdk.protocol.ServerMessage

public data class NetplayClientState(
    public val room: RoomView? = null,
    public val assignedPlayerIndex: Int? = null,
    public val latestEventSeq: Long = 0,
    public val roomEpoch: Long = 0,
    public val sessionEpoch: Long = 0,
    public val lastError: NetplayCloseReason.RelayError? = null,
)

public class RoomStateMachine(
    public val reconnectTokens: ReconnectTokenStore = ReconnectTokenStore(),
    public val pause: PauseCoordinator = PauseCoordinator(),
    public val heartbeat: HeartbeatTracker = HeartbeatTracker(),
    public val frameClock: FrameClockTracker = FrameClockTracker(),
) {
    public var state: NetplayClientState = NetplayClientState()
        private set

    public fun apply(message: ServerMessage): NetplayClientState {
        when (message) {
            is ServerMessage.RoomJoined -> {
                reconnectTokens.apply(message)
                updateRoom(message.room, message.yourPlayerIndex)
            }
            is ServerMessage.RoomStateChanged -> updateRoom(message.room)
            is ServerMessage.CompatibilityRequested -> updateRoom(message.room)
            is ServerMessage.RecoveryStarted -> updateRoom(message.room)
            is ServerMessage.PlayerReconnected -> updateRoom(message.room)
            is ServerMessage.PlayerExited -> updateRoom(message.room)
            is ServerMessage.RecoveryResyncRequired -> updateRoom(message.room)
            is ServerMessage.StateHashMismatch -> updateRoom(message.room)
            is ServerMessage.InputDelayChanged -> updateRoom(message.room)
            is ServerMessage.StartSession -> updateRoom(message.room)
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
            is ServerMessage.Error -> {
                state = state.copy(lastError = NetplayCloseReason.RelayError(message.code, message.message))
            }
            is ServerMessage.InputFrameMessage -> frameClock.markPeerInputFrame(message.input)
            is ServerMessage.ServerFrameMessage -> frameClock.applyServerFrame(message.frame)
            ServerMessage.Pong,
            is ServerMessage.LinkCablePacketMessage,
            is ServerMessage.SnapshotChunkMessage,
            is ServerMessage.SnapshotComplete -> Unit
        }

        return state
    }

    private fun updateRoom(room: RoomView, assignedPlayerIndex: Int? = state.assignedPlayerIndex) {
        reconnectTokens.updateAcceptedEpoch(room.roomEpoch)
        frameClock.applyRoom(room)
        state = state.copy(
            room = room,
            assignedPlayerIndex = assignedPlayerIndex,
            latestEventSeq = room.eventSeq,
            roomEpoch = room.roomEpoch,
            sessionEpoch = room.sessionEpoch,
            lastError = null,
        )
    }

    private fun updateEpochs(eventSeq: Long, roomEpoch: Long, sessionEpoch: Long) {
        reconnectTokens.updateAcceptedEpoch(roomEpoch)
        state = state.copy(
            latestEventSeq = eventSeq,
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
        )
    }

    public fun reset() {
        reconnectTokens.clear()
        pause.reset()
        frameClock.reset()
        state = NetplayClientState()
    }
}
