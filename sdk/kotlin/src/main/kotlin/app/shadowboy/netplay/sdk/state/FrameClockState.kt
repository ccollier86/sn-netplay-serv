package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.InputFrame
import app.shadowboy.netplay.sdk.protocol.RoomFrameClockView
import app.shadowboy.netplay.sdk.protocol.RoomView
import app.shadowboy.netplay.sdk.protocol.ServerFrameRelease

public data class FrameClockPolicy(
    public val catchUpFrames: Long = 4,
    public val stallFrames: Long = 12,
)

public data class FrameClockDiagnostics(
    public val canonicalFrame: Long,
    public val catchUp: Boolean,
    public val localFrame: Long?,
    public val peerReadFrame: Long?,
    public val roomFrame: Long,
    public val serverFrame: Long,
    public val stalled: Boolean,
)

public class FrameClockTracker(
    private val policy: FrameClockPolicy = FrameClockPolicy(),
) {
    private var canonicalFrame: Long = 0
    private var localFrame: Long? = null
    private var peerReadFrame: Long? = null
    private var roomFrame: Long = 0
    private var serverFrame: Long = 0

    public fun applyRoom(room: RoomView) {
        applyFrameClockView(room.frameClock)
    }

    public fun applyFrameClockView(frameClock: RoomFrameClockView) {
        canonicalFrame = maxOf(canonicalFrame, frameClock.canonicalFrame)
        roomFrame = canonicalFrame
        frameClock.releasedFrame?.let { releasedFrame ->
            serverFrame = maxOf(serverFrame, releasedFrame)
        }
    }

    public fun applyServerFrame(frame: ServerFrameRelease): FrameClockDiagnostics {
        serverFrame = maxOf(serverFrame, frame.frame)
        canonicalFrame = maxOf(canonicalFrame, frame.canonicalFrame)
        roomFrame = canonicalFrame
        return snapshot()
    }

    public fun markLocalFrame(frame: Long): FrameClockDiagnostics {
        localFrame = maxOf(localFrame ?: 0, frame)
        return snapshot()
    }

    public fun markPeerInputFrame(input: InputFrame): FrameClockDiagnostics {
        peerReadFrame = maxOf(peerReadFrame ?: 0, input.frame)
        return snapshot()
    }

    public fun snapshot(): FrameClockDiagnostics {
        val local = localFrame
        return FrameClockDiagnostics(
            canonicalFrame = canonicalFrame,
            catchUp = local != null && serverFrame - local > policy.catchUpFrames,
            localFrame = local,
            peerReadFrame = peerReadFrame,
            roomFrame = roomFrame,
            serverFrame = serverFrame,
            stalled = local != null && local - serverFrame > policy.stallFrames,
        )
    }

    public fun reset() {
        canonicalFrame = 0
        localFrame = null
        peerReadFrame = null
        roomFrame = 0
        serverFrame = 0
    }
}
