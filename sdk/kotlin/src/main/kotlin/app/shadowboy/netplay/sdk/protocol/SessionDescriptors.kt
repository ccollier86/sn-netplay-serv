package app.shadowboy.netplay.sdk.protocol

import app.shadowboy.netplay.sdk.NETPLAY_PROTOCOL_VERSION
import kotlinx.serialization.Serializable

@Serializable
public data class CreateRoomRequest(
    public val desktopProtocolVersion: Int = NETPLAY_PROTOCOL_VERSION,
    public val session: NetplaySessionDescriptor,
)

@Serializable
public data class CreateRoomResponse(
    public val room: RoomView,
)

@Serializable
public data class RoomStatusResponse(
    public val room: RoomView,
)

@Serializable
public data class NetplaySessionDescriptor(
    public val hostClientKind: NetplayClientKind? = null,
    public val hostAppVersion: String? = null,
    public val roomMode: NetplayRoomMode = NetplayRoomMode.DirectInvite,
    public val mode: NetplaySessionMode = NetplaySessionMode.ControllerNetplay,
    public val game: NetplayGameDescriptor,
    public val core: NetplayCoreDescriptor,
    public val controller: ControllerNetplayDescriptor = ControllerNetplayDescriptor(),
    public val link: LinkCableDescriptor? = null,
    public val voice: NetplayVoiceDescriptor? = null,
    public val romIdentity: RomIdentity? = null,
    public val romRelayIntent: RomRelayIntent = RomRelayIntent.ExactMatchOnly,
    public val romRelay: RomRelayCapability? = null,
)

@Serializable
public data class RomIdentity(
    public val system: String,
    public val coreId: String,
    public val contentHash: String,
    public val sizeBytes: Long,
    public val fileName: String? = null,
    public val extension: String? = null,
    public val displayName: String,
)

@Serializable
public data class RomRelayCapability(
    public val supported: Boolean,
    public val available: Boolean,
    public val temporaryAccessOnly: Boolean,
    public val maxBytes: Long,
    public val allowedSystems: List<String> = emptyList(),
    public val reason: RomRelayCapabilityReason? = null,
)

@Serializable
public data class NetplayGameDescriptor(
    public val systemId: String,
    public val title: String,
    public val romSha256: String,
    public val contentKey: String,
    public val region: String? = null,
    public val revision: String? = null,
    public val discId: String? = null,
)

@Serializable
public data class NetplayCoreDescriptor(
    public val coreId: String,
    public val coreName: String? = null,
    public val coreVersion: String? = null,
    public val coreOptionsSha256: String? = null,
    public val stateFormat: String? = null,
)

@Serializable
public data class ControllerNetplayDescriptor(
    public val inputDelayFrames: Int = 3,
)

@Serializable
public data class LinkCableDescriptor(
    public val systemFamily: String,
    public val linkProtocol: String,
    public val runtimeProfile: String,
    public val maxPlayers: Int = 2,
    public val transport: LinkCableTransport = LinkCableTransport.Relay,
)

@Serializable
public data class NetplayVoiceDescriptor(
    public val enabled: Boolean = false,
    public val mode: NetplayVoiceMode = NetplayVoiceMode.VoiceActivation,
)

public fun NetplaySessionDescriptor.validateForRelay() {
    require(game.romSha256.isSha256Hex()) { "game.romSha256 must be 64 hex characters" }
    require(controller.inputDelayFrames in 1..8) {
        "controller.inputDelayFrames must be in 1..8"
    }
    if (mode == NetplaySessionMode.LinkCable) {
        require(link != null) { "link descriptor is required for linkCable rooms" }
    } else {
        require(link == null) { "link descriptor is only valid for linkCable rooms" }
    }
    romIdentity?.let { identity ->
        require(identity.contentHash.isContentHash()) {
            "romIdentity.contentHash must be a SHA-256 digest"
        }
        require(identity.sizeBytes > 0L) { "romIdentity.sizeBytes must be positive" }
        require(identity.system == game.systemId) { "romIdentity.system must match game.systemId" }
        require(identity.coreId == core.coreId) { "romIdentity.coreId must match core.coreId" }
        require(identity.contentHash.normalizedContentHash().equals(game.romSha256, ignoreCase = true)) {
            "romIdentity.contentHash must match game.romSha256"
        }
    }
}

private fun String.isSha256Hex(): Boolean =
    length == 64 && all { value -> value in '0'..'9' || value in 'a'..'f' || value in 'A'..'F' }

private fun String.isContentHash(): Boolean =
    isSha256Hex() || removePrefix("sha256:").isSha256Hex()

private fun String.normalizedContentHash(): String =
    removePrefix("sha256:").lowercase()
