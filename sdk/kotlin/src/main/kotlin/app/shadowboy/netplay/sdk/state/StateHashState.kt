package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.ClientMessage
import app.shadowboy.netplay.sdk.protocol.StateHashReport

private val SHA256_PATTERN = Regex("^[a-f0-9]{64}$")

public data class StateHashReporterPolicy(
    public val reportEveryFrames: Long = 60,
)

public class StateHashReporter(
    policy: StateHashReporterPolicy = StateHashReporterPolicy(),
) {
    private var lastSubmittedFrame: Long? = null
    private val reportEveryFrames: Long = policy.reportEveryFrames.coerceAtLeast(1)

    public fun shouldReport(frame: Long): Boolean {
        require(frame >= 0) { "Netplay state hash frame must be non-negative." }
        if (lastSubmittedFrame == frame) {
            return false
        }

        val lastFrame = lastSubmittedFrame
        if (lastFrame == null) {
            return frame == 0L || frame >= reportEveryFrames
        }

        return frame - lastFrame >= reportEveryFrames
    }

    public fun stateHashMessage(
        roomEpoch: Long,
        sessionEpoch: Long,
        frame: Long,
        sha256: String,
    ): ClientMessage.StateHash {
        require(frame >= 0) { "Netplay state hash frame must be non-negative." }
        val normalizedHash = normalizeSha256(sha256)

        lastSubmittedFrame = frame

        return ClientMessage.StateHash(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            report = StateHashReport(frame = frame, sha256 = normalizedHash),
        )
    }

    public fun reset() {
        lastSubmittedFrame = null
    }
}

public fun normalizeSha256(value: String): String {
    val normalized = value.trim().lowercase()
    require(SHA256_PATTERN.matches(normalized)) {
        "Netplay state hash must be a lowercase SHA-256 hex value."
    }

    return normalized
}
