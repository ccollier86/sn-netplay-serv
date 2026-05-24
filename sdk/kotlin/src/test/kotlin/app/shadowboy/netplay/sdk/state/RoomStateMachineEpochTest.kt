package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.json.NetplayJson
import app.shadowboy.netplay.sdk.protocol.InputFrame
import app.shadowboy.netplay.sdk.protocol.RoomView
import app.shadowboy.netplay.sdk.protocol.ServerMessage
import app.shadowboy.netplay.sdk.protocol.SessionPauseReason
import app.shadowboy.netplay.sdk.protocol.SessionPauseState
import app.shadowboy.netplay.sdk.protocol.SessionPauseView
import app.shadowboy.netplay.sdk.protocol.SnapshotChunk
import app.shadowboy.netplay.sdk.protocol.SnapshotManifest
import app.shadowboy.netplay.sdk.protocol.roomJson
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class RoomStateMachineEpochTest {
    @Test
    fun `stale epoch room messages are ignored`() {
        val stateMachine = joinedStateMachine()

        stateMachine.apply(
            ServerMessage.RoomStateChanged(
                eventSeq = 2,
                roomEpoch = 2,
                sessionEpoch = 3,
                room = room(status = "waitingForGuest", eventSeq = 2, roomEpoch = 2, sessionEpoch = 3),
            ),
        )

        assertEquals(5, stateMachine.state.latestEventSeq)
        assertEquals(3, stateMachine.state.roomEpoch)
    }

    @Test
    fun `stale same epoch room messages are ignored`() {
        val stateMachine = joinedStateMachine()

        stateMachine.apply(
            ServerMessage.RoomStateChanged(
                eventSeq = 7,
                roomEpoch = 3,
                sessionEpoch = 4,
                room = room(status = "waitingForGuest", eventSeq = 7, roomEpoch = 3, sessionEpoch = 4),
            ),
        )
        stateMachine.apply(
            ServerMessage.RoomStateChanged(
                eventSeq = 6,
                roomEpoch = 3,
                sessionEpoch = 4,
                room = room(status = "waitingForGuest", eventSeq = 6, roomEpoch = 3, sessionEpoch = 4),
            ),
        )

        assertEquals(7, stateMachine.state.latestEventSeq)
    }

    @Test
    fun `stale input frames are ignored by epoch`() {
        val stateMachine = joinedStateMachine()

        stateMachine.apply(
            ServerMessage.InputFrameMessage(
                roomEpoch = 2,
                sessionEpoch = 4,
                input = InputFrame(playerIndex = 1, frame = 16, payload = listOf(1)),
            ),
        )
        assertNull(stateMachine.frameClock.snapshot().peerReadFrame)

        stateMachine.apply(
            ServerMessage.InputFrameMessage(
                roomEpoch = 3,
                sessionEpoch = 4,
                input = InputFrame(playerIndex = 1, frame = 17, payload = listOf(1)),
            ),
        )
        assertEquals(17, stateMachine.frameClock.snapshot().peerReadFrame)
    }

    @Test
    fun `snapshot runtime messages require the exact active epoch`() {
        val stateMachine = joinedStateMachine()

        assertFalse(
            stateMachine.isRuntimeMessageCurrent(
                ServerMessage.SnapshotChunkMessage(
                    roomEpoch = 2,
                    sessionEpoch = 4,
                    chunk = SnapshotChunk(snapshotId = "snapshot-1", repairFrame = 0, index = 0, bytes = listOf(1)),
                ),
            ),
        )
        assertFalse(
            stateMachine.isRuntimeMessageCurrent(
                ServerMessage.SnapshotComplete(
                    roomEpoch = 3,
                    sessionEpoch = 5,
                    manifest = SnapshotManifest(
                        snapshotId = "snapshot-1",
                        repairFrame = 0,
                        totalBytes = 1,
                        sha256 = "a".repeat(64),
                    ),
                ),
            ),
        )
        assertTrue(
            stateMachine.isRuntimeMessageCurrent(
                ServerMessage.SnapshotChunkMessage(
                    roomEpoch = 3,
                    sessionEpoch = 4,
                    chunk = SnapshotChunk(snapshotId = "snapshot-1", repairFrame = 0, index = 0, bytes = listOf(1)),
                ),
            ),
        )
    }

    @Test
    fun `diagnostics exposes effective pause and frame health`() {
        val stateMachine = RoomStateMachine()
        stateMachine.apply(
            ServerMessage.RoomJoined(
                eventSeq = 1,
                roomEpoch = 2,
                sessionEpoch = 3,
                yourPlayerIndex = 0,
                resumeToken = "token",
                inputSocketToken = "input-token",
                room = room(status = "waitingForGuest", eventSeq = 1, roomEpoch = 2, sessionEpoch = 3),
            ),
        )
        stateMachine.apply(
            ServerMessage.SessionPauseScheduled(
                eventSeq = 2,
                roomEpoch = 2,
                sessionEpoch = 3,
                pause = SessionPauseView(
                    sequence = 1,
                    state = SessionPauseState.Pausing,
                    reason = SessionPauseReason.Menu,
                    requestedByPlayerIndex = 1,
                    pauseAtFrame = 50,
                    pausedAtFrame = null,
                    acknowledgedPlayerIndexes = emptyList(),
                    holders = emptyList(),
                ),
                room = room(status = "waitingForGuest", eventSeq = 2, roomEpoch = 2, sessionEpoch = 3),
            ),
        )

        val diagnostics = stateMachine.diagnostics(nowMs = 1_000)

        assertEquals(0, diagnostics.assignedPlayerIndex)
        assertEquals(NetplayEffectivePauseReason.Peer, diagnostics.effectivePauseReason)
        assertEquals(HeartbeatHealth.Fresh, diagnostics.heartbeat)
        assertEquals(true, diagnostics.reconnectTicketAvailable)
    }

    private fun joinedStateMachine(): RoomStateMachine {
        val stateMachine = RoomStateMachine()
        stateMachine.apply(
            ServerMessage.RoomJoined(
                eventSeq = 5,
                roomEpoch = 3,
                sessionEpoch = 4,
                yourPlayerIndex = 0,
                resumeToken = "token",
                inputSocketToken = "input-token",
                room = room(status = "waitingForGuest", eventSeq = 5, roomEpoch = 3, sessionEpoch = 4),
            ),
        )
        return stateMachine
    }

    private fun room(
        status: String,
        eventSeq: Long,
        roomEpoch: Long,
        sessionEpoch: Long,
    ): RoomView = NetplayJson.format.decodeFromString(
        RoomView.serializer(),
        roomJson(
            status = status,
            eventSeq = eventSeq,
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
        ),
    )
}
