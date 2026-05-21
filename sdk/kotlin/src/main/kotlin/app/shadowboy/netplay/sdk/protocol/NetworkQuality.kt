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
)
