package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.ClientMessage
import app.shadowboy.netplay.sdk.protocol.ClientNetworkQualityReport
import app.shadowboy.netplay.sdk.protocol.ClientRuntimeState
import app.shadowboy.netplay.sdk.protocol.ServerMessage
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

public enum class HeartbeatHealth {
    Fresh,
    Stale,
    RecoveryTimedOut,
}

public data class HeartbeatPolicy(
    public val staleAfter: Duration = 15.seconds,
    public val recoveryAfter: Duration = 30.seconds,
)

public class HeartbeatTracker(
    private val policy: HeartbeatPolicy = HeartbeatPolicy(),
) {
    private var lastAckMillis: Long? = null
    private var lastAckEventSeq: Long? = null

    public fun markAck(message: ServerMessage.HeartbeatAck, nowMillis: Long) {
        lastAckEventSeq = message.eventSeq
        lastAckMillis = nowMillis
    }

    public fun health(nowMillis: Long): HeartbeatHealth {
        val lastAck = lastAckMillis ?: return HeartbeatHealth.Fresh
        val age = (nowMillis - lastAck).coerceAtLeast(0)

        return when {
            age >= policy.recoveryAfter.inWholeMilliseconds -> HeartbeatHealth.RecoveryTimedOut
            age >= policy.staleAfter.inWholeMilliseconds -> HeartbeatHealth.Stale
            else -> HeartbeatHealth.Fresh
        }
    }

    public fun lastAck(): HeartbeatAckState =
        HeartbeatAckState(
            eventSeq = lastAckEventSeq,
            receivedAtMs = lastAckMillis,
        )

    public fun heartbeatMessage(
        roomEpoch: Long,
        sessionEpoch: Long,
        latestEventSeq: Long,
        localFrame: Long?,
        runtimeState: ClientRuntimeState,
        network: ClientNetworkQualityReport? = null,
        telemetry: RuntimeTelemetryTracker? = null,
    ): ClientMessage.Heartbeat =
        telemetry?.consume().let { telemetrySnapshot ->
            ClientMessage.Heartbeat(
                roomEpoch = roomEpoch,
                sessionEpoch = sessionEpoch,
                latestEventSeq = latestEventSeq,
                localFrame = localFrame ?: telemetrySnapshot?.localFrame,
                runtimeState = runtimeState,
                network = network ?: telemetrySnapshot?.network,
            )
        }
}

public data class HeartbeatAckState(
    public val eventSeq: Long?,
    public val receivedAtMs: Long?,
)
