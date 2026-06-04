package app.shadowboy.netplay.sdk.protocol

import kotlinx.serialization.Serializable

@Serializable
public data class SnapshotChunk(
    public val snapshotId: String,
    public val repairFrame: Long,
    public val index: Int,
    public val bytes: List<Int>,
)

@Serializable
public data class SnapshotManifest(
    public val snapshotId: String,
    public val repairFrame: Long,
    public val totalBytes: Long,
    public val sha256: String,
)

@Serializable
public data class SnapshotFileRelayGrant(
    public val transferId: String,
    public val relayUrl: String,
    public val token: String,
    public val role: SnapshotFileRelayGrantRole,
    public val chunkSizeBytes: Long,
    public val chunkCount: Long,
    public val expiresAt: String,
    public val manifest: SnapshotManifest,
)

@Serializable
public data class RomRelayGrant(
    public val transferId: String,
    public val relayUrl: String,
    public val token: String,
    public val role: RomRelayGrantRole,
    public val rom: RomIdentity,
    public val senderPlayerIndex: Int,
    public val receiverPlayerIndex: Int,
    public val chunkSizeBytes: Long,
    public val chunkCount: Long,
    public val expiresAt: String,
)

@Serializable
public data class RomRelayProgress(
    public val transferId: String,
    public val role: RomRelayGrantRole,
    public val bytesComplete: Long,
    public val sizeBytes: Long,
)

@Serializable
public data class RomRelayCompletion(
    public val transferId: String,
    public val role: RomRelayGrantRole,
    public val contentHash: String,
)

@Serializable
public data class RomRelayFailure(
    public val transferId: String? = null,
    public val reason: RomRelayFailReason,
)

@Serializable
public data class RomRelayBlocked(
    public val reason: RomRelayBlockReason,
)

@Serializable
public data class RomRelayCancelled(
    public val transferId: String? = null,
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
public data class NearbyStateHashMatchView(
    public val sourcePlayerIndex: Int,
    public val sourceFrame: Long,
    public val matchedPlayerIndex: Int,
    public val matchedFrame: Long,
    public val frameOffset: Long,
)

@Serializable
public data class StateHashMismatchView(
    public val frame: Long,
    public val repairFrame: Long,
    public val hashes: List<PlayerStateHashView>,
    public val nearbyMatches: List<NearbyStateHashMatchView> = emptyList(),
)
