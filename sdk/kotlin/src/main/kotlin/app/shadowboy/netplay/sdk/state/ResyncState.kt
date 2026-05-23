package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.PlayerRole
import app.shadowboy.netplay.sdk.protocol.RoomView
import app.shadowboy.netplay.sdk.protocol.ServerMessage
import app.shadowboy.netplay.sdk.protocol.StateHashMismatchView

public enum class NetplayResyncReason {
    StateHashMismatch,
    Recovery,
}

public enum class NetplayResyncPhase {
    Requested,
    Pausing,
    SnapshotNeeded,
    SnapshotSending,
    SnapshotReceiving,
    LoadingSnapshot,
    WaitingForCompatibility,
    WaitingForReady,
    Complete,
    Failed,
}

public enum class NetplayResyncRole {
    Host,
    Guest,
    Unknown,
}

public data class NetplayResyncState(
    public val reason: NetplayResyncReason,
    public val phase: NetplayResyncPhase,
    public val eventSeq: Long,
    public val roomEpoch: Long,
    public val sessionEpoch: Long,
    public val assignedPlayerIndex: Int?,
    public val hostPlayerIndex: Int?,
    public val role: NetplayResyncRole,
    public val mustSendSnapshot: Boolean,
    public val mustLoadSnapshot: Boolean,
    public val requestedAtMs: Long,
    public val updatedAtMs: Long,
    public val mismatch: StateHashMismatchView? = null,
    public val failureReason: String? = null,
)

public data class ResyncContext(
    public val assignedPlayerIndex: Int? = null,
    public val nowMs: Long = System.currentTimeMillis(),
)

public class ResyncCoordinator {
    public var currentResync: NetplayResyncState? = null
        private set

    public fun apply(message: ServerMessage.StateHashMismatch, context: ResyncContext = ResyncContext()) {
        currentResync = buildState(
            reason = NetplayResyncReason.StateHashMismatch,
            eventSeq = message.eventSeq,
            roomEpoch = message.roomEpoch,
            sessionEpoch = message.sessionEpoch,
            room = message.room,
            mismatch = message.mismatch,
            context = context,
        )
    }

    public fun apply(message: ServerMessage.RecoveryResyncRequired, context: ResyncContext = ResyncContext()) {
        currentResync = buildState(
            reason = NetplayResyncReason.Recovery,
            eventSeq = message.eventSeq,
            roomEpoch = message.roomEpoch,
            sessionEpoch = message.sessionEpoch,
            room = message.room,
            mismatch = null,
            context = context,
        )
    }

    public fun markPausing(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.Pausing, nowMs)
    }

    public fun markSnapshotNeeded(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.SnapshotNeeded, nowMs)
    }

    public fun markSnapshotSendStarted(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.SnapshotSending, nowMs)
    }

    public fun markSnapshotSendComplete(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.WaitingForCompatibility, nowMs)
    }

    public fun markSnapshotReceiveStarted(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.SnapshotReceiving, nowMs)
    }

    public fun markSnapshotLoadStarted(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.LoadingSnapshot, nowMs)
    }

    public fun markSnapshotLoadComplete(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.WaitingForCompatibility, nowMs)
    }

    public fun markCompatibilitySent(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.WaitingForReady, nowMs)
    }

    public fun markComplete(nowMs: Long = System.currentTimeMillis()) {
        transition(NetplayResyncPhase.Complete, nowMs)
    }

    public fun markFailed(reason: String, nowMs: Long = System.currentTimeMillis()) {
        currentResync = currentResync?.copy(
            phase = NetplayResyncPhase.Failed,
            failureReason = reason,
            updatedAtMs = nowMs,
        )
    }

    public fun shouldPauseEmulation(): Boolean =
        currentResync?.phase?.let { it !in TERMINAL_PHASES } ?: false

    public fun shouldClearPredictionBuffers(): Boolean =
        currentResync?.phase == NetplayResyncPhase.Requested

    public fun shouldSendHostSnapshot(): Boolean =
        currentResync?.let { it.mustSendSnapshot && it.phase == NetplayResyncPhase.SnapshotNeeded } ?: false

    public fun shouldWaitForSnapshot(): Boolean =
        currentResync?.let {
            it.mustLoadSnapshot &&
                (it.phase == NetplayResyncPhase.SnapshotNeeded || it.phase == NetplayResyncPhase.SnapshotReceiving)
        } ?: false

    public fun shouldRequestCompatibility(): Boolean =
        currentResync?.let {
            it.phase !in TERMINAL_PHASES &&
                (
                    it.phase == NetplayResyncPhase.Requested ||
                        it.phase == NetplayResyncPhase.Pausing ||
                        it.phase == NetplayResyncPhase.WaitingForCompatibility
                    )
        } ?: false

    public fun shouldSendReady(): Boolean =
        currentResync?.phase == NetplayResyncPhase.WaitingForReady

    public fun clear() {
        currentResync = null
    }

    public fun reset() {
        clear()
    }

    private fun transition(phase: NetplayResyncPhase, nowMs: Long) {
        currentResync = currentResync?.copy(phase = phase, updatedAtMs = nowMs)
    }

    private fun buildState(
        reason: NetplayResyncReason,
        eventSeq: Long,
        roomEpoch: Long,
        sessionEpoch: Long,
        room: RoomView,
        mismatch: StateHashMismatchView?,
        context: ResyncContext,
    ): NetplayResyncState {
        val hostPlayerIndex = room.players.firstOrNull { it.role == PlayerRole.Host }?.playerIndex
        val role = resyncRole(context.assignedPlayerIndex, hostPlayerIndex)

        return NetplayResyncState(
            reason = reason,
            phase = NetplayResyncPhase.Requested,
            eventSeq = eventSeq,
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            assignedPlayerIndex = context.assignedPlayerIndex,
            hostPlayerIndex = hostPlayerIndex,
            role = role,
            mustSendSnapshot = role == NetplayResyncRole.Host,
            mustLoadSnapshot = role == NetplayResyncRole.Guest || role == NetplayResyncRole.Unknown,
            requestedAtMs = context.nowMs,
            updatedAtMs = context.nowMs,
            mismatch = mismatch,
        )
    }
}

private val TERMINAL_PHASES: Set<NetplayResyncPhase> =
    setOf(NetplayResyncPhase.Complete, NetplayResyncPhase.Failed)

private fun resyncRole(assignedPlayerIndex: Int?, hostPlayerIndex: Int?): NetplayResyncRole =
    when {
        assignedPlayerIndex == null || hostPlayerIndex == null -> NetplayResyncRole.Unknown
        assignedPlayerIndex == hostPlayerIndex -> NetplayResyncRole.Host
        else -> NetplayResyncRole.Guest
    }
