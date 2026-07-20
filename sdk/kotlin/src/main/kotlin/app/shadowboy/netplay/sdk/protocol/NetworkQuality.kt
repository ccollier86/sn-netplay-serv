package app.shadowboy.netplay.sdk.protocol

import kotlinx.serialization.Serializable

@Serializable
public data class ClientNetworkQualityReport(
    public val roundTripMs: Int? = null,
    public val jitterMs: Int? = null,
    public val predictionFrames: Int? = null,
    public val stallCount: Int? = null,
    public val catchUpFrames: Int? = null,
    public val lateInputFrames: Int? = null,
    public val audioUnderruns: Int? = null,
    public val inputResendFrames: Int? = null,
    public val inputNacks: Int? = null,
    public val replayedFrames: Int? = null,
    public val suppressedAudioFrames: Int? = null,
    public val suppressedVideoFrames: Int? = null,
    public val audioQueueDepthFrames: Int? = null,
    public val audioCatchUpEvents: Int? = null,
    public val audioTrimmedFrames: Int? = null,
    public val audioRebufferEvents: Int? = null,
    public val audioMaxConsecutiveMissingFrames: Int? = null,
    public val audioQueueMinFrames: Int? = null,
    public val audioQueueMaxFrames: Int? = null,
    public val clockUncertaintyMs: Int? = null,
)
