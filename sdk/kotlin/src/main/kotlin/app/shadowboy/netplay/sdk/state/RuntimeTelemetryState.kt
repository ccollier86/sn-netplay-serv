package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.ClientNetworkQualityReport
import kotlin.math.abs
import kotlin.math.roundToInt

private const val MAX_TELEMETRY_VALUE: Int = 1_000_000
private const val MAX_LATENCY_MS: Int = 60_000

public data class RuntimeTelemetrySnapshot(
    public val localFrame: Long?,
    public val network: ClientNetworkQualityReport,
)

public class RuntimeTelemetryTracker {
    private var audioUnderruns: Int = 0
    private var catchUpFrames: Int = 0
    private var jitterMs: Double? = null
    private var lastRoundTripMs: Int? = null
    private var lateInputFrames: Int = 0
    private var localFrame: Long? = null
    private var predictionFrames: Int? = null
    private var roundTripMs: Int? = null
    private var stallCount: Int = 0

    public fun markLocalFrame(frame: Long) {
        require(frame >= 0) { "Netplay local frame must be non-negative." }
        localFrame = maxOf(localFrame ?: 0, frame)
    }

    public fun setPredictionFrames(frames: Int?) {
        predictionFrames = frames?.let(::clampTelemetryValue)
    }

    public fun recordRoundTrip(ms: Int) {
        val sample = clampTelemetryValue(ms, MAX_LATENCY_MS)
        val previous = lastRoundTripMs
        if (previous != null) {
            val delta = abs(sample - previous).toDouble()
            jitterMs = jitterMs?.let { jitter -> jitter + (delta - jitter) / 16.0 } ?: delta
        }

        lastRoundTripMs = sample
        roundTripMs = sample
    }

    public fun recordStall(count: Int = 1) {
        stallCount = addTelemetryCount(stallCount, count)
    }

    public fun recordCatchUpFrames(count: Int) {
        catchUpFrames = addTelemetryCount(catchUpFrames, count)
    }

    public fun recordLateInputFrames(count: Int) {
        lateInputFrames = addTelemetryCount(lateInputFrames, count)
    }

    public fun recordAudioUnderruns(count: Int = 1) {
        audioUnderruns = addTelemetryCount(audioUnderruns, count)
    }

    public fun snapshot(): RuntimeTelemetrySnapshot =
        RuntimeTelemetrySnapshot(
            localFrame = localFrame,
            network = networkReport(),
        )

    public fun consume(): RuntimeTelemetrySnapshot {
        val snapshot = snapshot()

        stallCount = 0
        catchUpFrames = 0
        lateInputFrames = 0
        audioUnderruns = 0

        return snapshot
    }

    public fun reset() {
        audioUnderruns = 0
        catchUpFrames = 0
        jitterMs = null
        lastRoundTripMs = null
        lateInputFrames = 0
        localFrame = null
        predictionFrames = null
        roundTripMs = null
        stallCount = 0
    }

    private fun networkReport(): ClientNetworkQualityReport =
        ClientNetworkQualityReport(
            audioUnderruns = audioUnderruns,
            catchUpFrames = catchUpFrames,
            jitterMs = jitterMs?.roundToInt(),
            lateInputFrames = lateInputFrames,
            predictionFrames = predictionFrames,
            roundTripMs = roundTripMs,
            stallCount = stallCount,
        )
}

private fun addTelemetryCount(current: Int, delta: Int): Int =
    clampTelemetryValue(current + clampTelemetryValue(delta))

private fun clampTelemetryValue(value: Int, max: Int = MAX_TELEMETRY_VALUE): Int =
    when {
        value <= 0 -> 0
        value > max -> max
        else -> value
    }
