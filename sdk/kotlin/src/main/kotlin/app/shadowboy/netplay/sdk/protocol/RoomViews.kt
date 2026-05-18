package app.shadowboy.netplay.sdk.protocol

import kotlinx.serialization.Serializable

@Serializable
public data class NetplayProtocolView(
    public val protocolVersion: Int,
    public val minSupportedProtocolVersion: Int,
)

@Serializable
public data class RoomView(
    public val roomId: String,
    public val eventSeq: Long,
    public val roomEpoch: Long,
    public val sessionEpoch: Long,
    public val inviteCode: String,
    public val protocol: NetplayProtocolView,
    public val session: NetplaySessionDescriptor,
    public val maxPlayers: Int,
    public val pause: SessionPauseView? = null,
    public val status: RoomStatus,
    public val players: List<PlayerSlotView>,
)

@Serializable
public data class PlayerSlotView(
    public val playerIndex: Int,
    public val displayNumber: Int,
    public val role: PlayerRole,
    public val status: PlayerStatus,
    public val runtimeState: PlayerRuntimeState,
    public val occupied: Boolean,
    public val lastSeenAgeMs: Long? = null,
    public val reconnectGraceRemainingMs: Long? = null,
)

@Serializable
public data class SessionPauseHolder(
    public val playerIndex: Int,
    public val reason: SessionPauseReason,
)

@Serializable
public data class SessionPauseView(
    public val sequence: Long,
    public val state: SessionPauseState,
    public val reason: SessionPauseReason,
    public val requestedByPlayerIndex: Int,
    public val pauseAtFrame: Long,
    public val pausedAtFrame: Long? = null,
    public val acknowledgedPlayerIndexes: List<Int>,
    public val holders: List<SessionPauseHolder>,
)
