package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.ClientMessage
import app.shadowboy.netplay.sdk.protocol.RomRelayCancelled
import app.shadowboy.netplay.sdk.protocol.RomRelayCompletion
import app.shadowboy.netplay.sdk.protocol.RomRelayFailure
import app.shadowboy.netplay.sdk.protocol.RomRelayGrant
import app.shadowboy.netplay.sdk.protocol.RomRelayGrantRole
import app.shadowboy.netplay.sdk.protocol.RomRelayProgress
import app.shadowboy.netplay.sdk.protocol.ServerMessage

public sealed interface RomRelayEvent {
    public data class UploadGranted(public val grant: RomRelayGrant) : RomRelayEvent
    public data class DownloadGranted(public val grant: RomRelayGrant) : RomRelayEvent
    public data class ProgressChanged(public val progress: RomRelayProgress) : RomRelayEvent
    public data class Completed(public val completion: RomRelayCompletion) : RomRelayEvent
    public data class Blocked(public val blocked: app.shadowboy.netplay.sdk.protocol.RomRelayBlocked) : RomRelayEvent
    public data class Failed(public val failure: RomRelayFailure) : RomRelayEvent
    public data class Cancelled(public val cancelled: RomRelayCancelled) : RomRelayEvent
}

public object RomRelayCommands {
    public fun requestRomRelay(
        roomId: String,
        roomEpoch: Long,
        sessionEpoch: Long,
    ): ClientMessage.RomRelayRequest {
        require(roomId.isNotBlank()) { "roomId is required" }
        return ClientMessage.RomRelayRequest(roomEpoch = roomEpoch, sessionEpoch = sessionEpoch)
    }

    public fun ackRomRelayUploaded(
        roomId: String,
        roomEpoch: Long,
        sessionEpoch: Long,
        transferId: String,
        contentHash: String,
    ): ClientMessage.RomRelayCompleted =
        complete(roomId, roomEpoch, sessionEpoch, transferId, contentHash, RomRelayGrantRole.Upload)

    public fun ackRomRelayDownloaded(
        roomId: String,
        roomEpoch: Long,
        sessionEpoch: Long,
        transferId: String,
        contentHash: String,
    ): ClientMessage.RomRelayCompleted =
        complete(roomId, roomEpoch, sessionEpoch, transferId, contentHash, RomRelayGrantRole.Download)

    public fun reportRomRelayProgress(
        roomId: String,
        roomEpoch: Long,
        sessionEpoch: Long,
        progress: RomRelayProgress,
    ): ClientMessage.RomRelayProgressMessage {
        require(roomId.isNotBlank()) { "roomId is required" }
        return ClientMessage.RomRelayProgressMessage(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            progress = progress,
        )
    }

    public fun failRomRelay(
        roomId: String,
        roomEpoch: Long,
        sessionEpoch: Long,
        failure: RomRelayFailure,
    ): ClientMessage.RomRelayFailed {
        require(roomId.isNotBlank()) { "roomId is required" }
        return ClientMessage.RomRelayFailed(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            failure = failure,
        )
    }

    public fun cancelRomRelay(
        roomId: String,
        roomEpoch: Long,
        sessionEpoch: Long,
        transferId: String? = null,
    ): ClientMessage.RomRelayCancelledMessage {
        require(roomId.isNotBlank()) { "roomId is required" }
        return ClientMessage.RomRelayCancelledMessage(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            cancelled = RomRelayCancelled(transferId = transferId),
        )
    }

    private fun complete(
        roomId: String,
        roomEpoch: Long,
        sessionEpoch: Long,
        transferId: String,
        contentHash: String,
        role: RomRelayGrantRole,
    ): ClientMessage.RomRelayCompleted {
        require(roomId.isNotBlank()) { "roomId is required" }
        require(transferId.isNotBlank()) { "transferId is required" }
        require(contentHash.isNotBlank()) { "contentHash is required" }
        return ClientMessage.RomRelayCompleted(
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
            completion = RomRelayCompletion(
                transferId = transferId,
                role = role,
                contentHash = contentHash,
            ),
        )
    }
}

public fun ServerMessage.toRomRelayEventOrNull(): RomRelayEvent? =
    when (this) {
        is ServerMessage.RomRelayGrantUpload -> RomRelayEvent.UploadGranted(grant)
        is ServerMessage.RomRelayGrantDownload -> RomRelayEvent.DownloadGranted(grant)
        is ServerMessage.RomRelayProgressChanged -> RomRelayEvent.ProgressChanged(progress)
        is ServerMessage.RomRelayCompleted -> RomRelayEvent.Completed(completion)
        is ServerMessage.RomRelayBlockedMessage -> RomRelayEvent.Blocked(blocked)
        is ServerMessage.RomRelayFailed -> RomRelayEvent.Failed(failure)
        is ServerMessage.RomRelayCancelledMessage -> RomRelayEvent.Cancelled(cancelled)
        else -> null
    }
