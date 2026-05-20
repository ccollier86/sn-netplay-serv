package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.ClientMessage
import app.shadowboy.netplay.sdk.protocol.SessionPauseReason
import app.shadowboy.netplay.sdk.protocol.SessionPauseView
import java.util.UUID

public class PauseCoordinator {
    public var currentPause: SessionPauseView? = null
        private set

    public fun apply(pause: SessionPauseView) {
        currentPause = pause
    }

    public fun clear(sequence: Long) {
        if (currentPause?.sequence == sequence) {
            currentPause = null
        }
    }

    public fun reset() {
        currentPause = null
    }

    public fun requestPause(
        roomEpoch: Long,
        sessionEpoch: Long,
        reason: SessionPauseReason,
        localFrame: Long,
        requestId: String = UUID.randomUUID().toString(),
    ): ClientMessage.RequestSessionPause =
        ClientMessage.RequestSessionPause(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            requestId = requestId,
            reason = reason,
            localFrame = localFrame,
        )

    public fun pauseReached(
        roomEpoch: Long,
        sessionEpoch: Long,
        pausedAtFrame: Long,
    ): ClientMessage.SessionPauseReached {
        val pause = requireNotNull(currentPause) { "No active pause to acknowledge" }

        return ClientMessage.SessionPauseReached(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            sequence = pause.sequence,
            pausedAtFrame = pausedAtFrame,
        )
    }

    public fun requestResume(
        roomEpoch: Long,
        sessionEpoch: Long,
        reason: SessionPauseReason,
        requestId: String = UUID.randomUUID().toString(),
    ): ClientMessage.RequestSessionResume {
        val pause = requireNotNull(currentPause) { "No active pause to resume" }

        return ClientMessage.RequestSessionResume(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            requestId = requestId,
            reason = reason,
            sequence = pause.sequence,
        )
    }
}
