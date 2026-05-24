package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.PlayerVoiceJoinGrant
import app.shadowboy.netplay.sdk.protocol.RoomView
import app.shadowboy.netplay.sdk.protocol.RoomVoiceStatus
import app.shadowboy.netplay.sdk.protocol.ServerMessage

public data class NetplayVoiceGrantState(
    public val privateGrant: PlayerVoiceJoinGrant? = null,
    public val roomAvailable: Boolean = false,
    public val refreshedAtEventSeq: Long? = null,
)

public data class NetplayVoiceDiagnostics(
    public val available: Boolean,
    public val grantAvailable: Boolean,
    public val participantIdentity: String?,
    public val expiresAt: String?,
)

/** Tracks private voice grants without exposing tokens in room diagnostics. */
public class NetplayVoiceGrantTracker {
    public var state: NetplayVoiceGrantState = NetplayVoiceGrantState()
        private set

    /** Applies a shared room view and clears stale grants when voice is unavailable. */
    public fun applyRoom(room: RoomView): NetplayVoiceGrantState {
        val roomAvailable = room.isVoiceAvailable()
        state = state.copy(
            privateGrant = if (roomAvailable) state.privateGrant else null,
            roomAvailable = roomAvailable,
        )
        return state
    }

    /** Applies private relay messages that carry voice grants for this player. */
    public fun applyMessage(message: ServerMessage): NetplayVoiceGrantState {
        when (message) {
            is ServerMessage.RoomJoined -> {
                applyRoom(message.room)
                state = state.copy(
                    privateGrant = message.voice ?: if (state.roomAvailable) state.privateGrant else null,
                )
            }
            is ServerMessage.VoiceTokenRefreshed -> {
                state = state.copy(
                    privateGrant = message.voice,
                    refreshedAtEventSeq = message.eventSeq,
                    roomAvailable = true,
                )
            }
            else -> Unit
        }

        return state
    }

    /** Returns token-safe diagnostics for UI and logs. */
    public fun diagnostics(): NetplayVoiceDiagnostics =
        state.diagnostics()

    public fun reset() {
        state = NetplayVoiceGrantState()
    }
}

public fun RoomView.isVoiceAvailable(): Boolean =
    voice?.status == RoomVoiceStatus.Available

public fun NetplayVoiceGrantState.diagnostics(): NetplayVoiceDiagnostics =
    NetplayVoiceDiagnostics(
        available = roomAvailable,
        grantAvailable = privateGrant != null,
        participantIdentity = privateGrant?.participantIdentity,
        expiresAt = privateGrant?.expiresAt,
    )
