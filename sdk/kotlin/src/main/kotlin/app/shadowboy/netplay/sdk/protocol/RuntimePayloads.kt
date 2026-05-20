package app.shadowboy.netplay.sdk.protocol

import kotlinx.serialization.Serializable

@Serializable
public data class SnapshotChunk(
    public val index: Int,
    public val bytes: List<Int>,
)

@Serializable
public data class SnapshotManifest(
    public val totalBytes: Long,
    public val sha256: String,
)

@Serializable
public data class InputFrame(
    public val playerIndex: Int,
    public val frame: Long,
    public val payload: List<Int>,
)

@Serializable
public data class LinkCablePacket(
    public val playerIndex: Int,
    public val sequence: Long,
    public val emulatedTime: Long,
    public val payload: List<Int>,
)

@Serializable
public data class StateHashReport(
    public val frame: Long,
    public val sha256: String,
)

@Serializable
public data class PlayerStateHashView(
    public val playerIndex: Int,
    public val sha256: String,
)

@Serializable
public data class StateHashMismatchView(
    public val frame: Long,
    public val hashes: List<PlayerStateHashView>,
)
