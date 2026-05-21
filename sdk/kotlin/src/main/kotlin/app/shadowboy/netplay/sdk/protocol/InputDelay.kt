package app.shadowboy.netplay.sdk.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
public enum class InputDelayChangeReason {
    @SerialName("initialLatency")
    InitialLatency,

    @SerialName("networkPressure")
    NetworkPressure,

    @SerialName("stableConnection")
    StableConnection,
}

@Serializable
public data class InputDelayChange(
    public val effectiveFrame: Long,
    public val inputDelayFrames: Int,
    public val previousInputDelayFrames: Int,
    public val reason: InputDelayChangeReason,
)
