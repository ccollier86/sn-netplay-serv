package app.shadowboy.netplay.sdk.protocol

import app.shadowboy.netplay.sdk.NETPLAY_PROTOCOL_VERSION
import kotlinx.serialization.Serializable

@Serializable
public data class CompatibilityFingerprint(
    public val desktopVersion: String,
    public val protocolVersion: Int = NETPLAY_PROTOCOL_VERSION,
    public val systemId: String,
    public val coreId: String,
    public val coreBuild: String,
    public val stateFormat: String? = null,
    public val contentHash: String,
    public val settingsHash: String,
    public val cheatsHash: String,
    public val systemDataHash: String? = null,
    public val saveDataMode: String = "netplay",
)

@Serializable
public data class LinkCableCompatibility(
    public val protocolVersion: Int = NETPLAY_PROTOCOL_VERSION,
    public val systemFamily: String,
    public val linkProtocol: String,
    public val runtimeProfile: String,
    public val systemDataHash: String? = null,
)

public object CompatibilityHashes {
    public const val SHA256_EMPTY: String =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
