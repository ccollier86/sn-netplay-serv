package app.shadowboy.netplay.sdk.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
public sealed interface ClientMessage {
    @Serializable
    @SerialName("ping")
    public data object Ping : ClientMessage

    @Serializable
    @SerialName("setCompatibilityFingerprint")
    public data class SetCompatibilityFingerprint(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val fingerprint: CompatibilityFingerprint,
    ) : ClientMessage

    @Serializable
    @SerialName("setLinkCableCompatibility")
    public data class SetLinkCableCompatibility(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val compatibility: LinkCableCompatibility,
    ) : ClientMessage

    @Serializable
    @SerialName("ready")
    public data class Ready(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val network: ClientNetworkQualityReport? = null,
    ) : ClientMessage

    @Serializable
    @SerialName("snapshotChunk")
    public data class SnapshotChunkMessage(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val chunk: SnapshotChunk,
    ) : ClientMessage

    @Serializable
    @SerialName("snapshotComplete")
    public data class SnapshotComplete(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val manifest: SnapshotManifest,
    ) : ClientMessage

    @Serializable
    @SerialName("inputFrame")
    public data class InputFrameMessage(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val input: InputFrame,
    ) : ClientMessage

    @Serializable
    @SerialName("linkCablePacket")
    public data class LinkCablePacketMessage(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val packet: LinkCablePacket,
    ) : ClientMessage

    @Serializable
    @SerialName("heartbeat")
    public data class Heartbeat(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val latestEventSeq: Long,
        public val localFrame: Long? = null,
        public val runtimeState: ClientRuntimeState,
        public val network: ClientNetworkQualityReport? = null,
    ) : ClientMessage

    @Serializable
    @SerialName("requestSessionPause")
    public data class RequestSessionPause(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val requestId: String,
        public val reason: SessionPauseReason,
        public val localFrame: Long,
    ) : ClientMessage

    @Serializable
    @SerialName("sessionPauseReached")
    public data class SessionPauseReached(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val sequence: Long,
        public val pausedAtFrame: Long,
    ) : ClientMessage

    @Serializable
    @SerialName("requestSessionResume")
    public data class RequestSessionResume(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val requestId: String,
        public val reason: SessionPauseReason,
        public val sequence: Long,
    ) : ClientMessage

    @Serializable
    @SerialName("playerExited")
    public data class PlayerExited(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val reason: String,
    ) : ClientMessage

    @Serializable
    @SerialName("stateHash")
    public data class StateHash(
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val report: StateHashReport,
    ) : ClientMessage
}

@Serializable
public sealed interface ServerMessage {
    public val eventSeqOrNull: Long?
    public val roomEpochOrNull: Long?
    public val sessionEpochOrNull: Long?

    @Serializable
    @SerialName("roomJoined")
    public data class RoomJoined(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val yourPlayerIndex: Int,
        public val resumeToken: String,
        public val inputSocketToken: String,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("roomStateChanged")
    public data class RoomStateChanged(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("compatibilityRequested")
    public data class CompatibilityRequested(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("recoveryStarted")
    public data class RecoveryStarted(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("playerReconnected")
    public data class PlayerReconnected(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val playerIndex: Int,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("playerExited")
    public data class PlayerExited(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val playerIndex: Int,
        public val reason: String,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("recoveryResyncRequired")
    public data class RecoveryResyncRequired(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("stateHashMismatch")
    public data class StateHashMismatch(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val mismatch: StateHashMismatchView,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("inputDelayChanged")
    public data class InputDelayChanged(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val change: InputDelayChange,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("startSession")
    public data class StartSession(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val startFrame: Long,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("sessionPauseScheduled")
    public data class SessionPauseScheduled(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val pause: SessionPauseView,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("sessionPauseUpdated")
    public data class SessionPauseUpdated(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val pause: SessionPauseView,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("sessionResumeScheduled")
    public data class SessionResumeScheduled(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
        public val sequence: Long,
        public val resumeAtFrame: Long,
        public val room: RoomView,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("heartbeatAck")
    public data class HeartbeatAck(
        public val eventSeq: Long,
        public val roomEpoch: Long,
        public val sessionEpoch: Long,
    ) : ServerMessage {
        override val eventSeqOrNull: Long = eventSeq
        override val roomEpochOrNull: Long = roomEpoch
        override val sessionEpochOrNull: Long = sessionEpoch
    }

    @Serializable
    @SerialName("inputFrame")
    public data class InputFrameMessage(public val input: InputFrame) : ServerMessage {
        override val eventSeqOrNull: Long? = null
        override val roomEpochOrNull: Long? = null
        override val sessionEpochOrNull: Long? = null
    }

    @Serializable
    @SerialName("serverFrame")
    public data class ServerFrameMessage(public val frame: ServerFrameRelease) : ServerMessage {
        override val eventSeqOrNull: Long? = null
        override val roomEpochOrNull: Long? = null
        override val sessionEpochOrNull: Long? = null
    }

    @Serializable
    @SerialName("linkCablePacket")
    public data class LinkCablePacketMessage(public val packet: LinkCablePacket) : ServerMessage {
        override val eventSeqOrNull: Long? = null
        override val roomEpochOrNull: Long? = null
        override val sessionEpochOrNull: Long? = null
    }

    @Serializable
    @SerialName("snapshotChunk")
    public data class SnapshotChunkMessage(public val chunk: SnapshotChunk) : ServerMessage {
        override val eventSeqOrNull: Long? = null
        override val roomEpochOrNull: Long? = null
        override val sessionEpochOrNull: Long? = null
    }

    @Serializable
    @SerialName("snapshotComplete")
    public data class SnapshotComplete(public val manifest: SnapshotManifest) : ServerMessage {
        override val eventSeqOrNull: Long? = null
        override val roomEpochOrNull: Long? = null
        override val sessionEpochOrNull: Long? = null
    }

    @Serializable
    @SerialName("pong")
    public data object Pong : ServerMessage {
        override val eventSeqOrNull: Long? = null
        override val roomEpochOrNull: Long? = null
        override val sessionEpochOrNull: Long? = null
    }

    @Serializable
    @SerialName("error")
    public data class Error(public val code: String, public val message: String) : ServerMessage {
        override val eventSeqOrNull: Long? = null
        override val roomEpochOrNull: Long? = null
        override val sessionEpochOrNull: Long? = null
    }
}
