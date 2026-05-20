package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.json.NetplayJson
import app.shadowboy.netplay.sdk.protocol.ClientRuntimeState
import app.shadowboy.netplay.sdk.protocol.InputFrame
import app.shadowboy.netplay.sdk.protocol.RoomStatus
import app.shadowboy.netplay.sdk.protocol.ServerMessage
import app.shadowboy.netplay.sdk.protocol.ServerFrameRelease
import app.shadowboy.netplay.sdk.protocol.SessionPauseReason
import app.shadowboy.netplay.sdk.protocol.SessionPauseState
import app.shadowboy.netplay.sdk.protocol.SessionPauseView
import app.shadowboy.netplay.sdk.protocol.roomJson
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.time.Duration.Companion.seconds

class RoomStateMachineTest {
    @Test
    fun `room joined stores reconnect ticket and assigned player`() {
        val stateMachine = RoomStateMachine()

        val state = stateMachine.apply(
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

        assertEquals(0, state.assignedPlayerIndex)
        assertEquals(2, stateMachine.reconnectTokens.current()?.roomEpoch)
        assertEquals("token", stateMachine.reconnectTokens.current()?.resumeToken)
    }

    @Test
    fun `recovery resync updates state and keeps assignment`() {
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

        val state = stateMachine.apply(
            ServerMessage.RecoveryResyncRequired(
                eventSeq = 10,
                roomEpoch = 5,
                sessionEpoch = 9,
                room = room(status = "checkingCompatibility", eventSeq = 10, roomEpoch = 5, sessionEpoch = 9),
            ),
        )

        assertEquals(0, state.assignedPlayerIndex)
        assertEquals(10, state.latestEventSeq)
        assertEquals(RoomStatus.CheckingCompatibility, state.room?.status)
        assertEquals(5, stateMachine.reconnectTokens.current()?.roomEpoch)
    }

    @Test
    fun `heartbeat tracker reports stale and recovery timeout`() {
        val tracker = HeartbeatTracker(
            HeartbeatPolicy(staleAfter = 5.seconds, recoveryAfter = 10.seconds),
        )
        tracker.markAck(ServerMessage.HeartbeatAck(1, 2, 3), nowMillis = 1_000)

        assertEquals(HeartbeatHealth.Fresh, tracker.health(nowMillis = 1_500))
        assertEquals(HeartbeatHealth.Stale, tracker.health(nowMillis = 6_000))
        assertEquals(HeartbeatHealth.RecoveryTimedOut, tracker.health(nowMillis = 11_000))
        assertEquals(
            ClientRuntimeState.Playing,
            tracker.heartbeatMessage(2, 3, 1, 24, ClientRuntimeState.Playing).runtimeState,
        )
    }

    @Test
    fun `pause coordinator creates and clears pause messages`() {
        val pause = PauseCoordinator()
        pause.apply(
            SessionPauseView(
                sequence = 3,
                state = SessionPauseState.Pausing,
                reason = SessionPauseReason.Menu,
                requestedByPlayerIndex = 0,
                pauseAtFrame = 120,
                pausedAtFrame = null,
                acknowledgedPlayerIndexes = emptyList(),
                holders = emptyList(),
            ),
        )

        assertEquals(3, pause.pauseReached(4, 5, pausedAtFrame = 121).sequence)
        assertEquals(3, pause.requestResume(4, 5, SessionPauseReason.Menu).sequence)

        pause.clear(3)
        assertNull(pause.currentPause)
    }

    @Test
    fun `frame clock tracks server frame and peer read frame`() {
        val stateMachine = RoomStateMachine()

        stateMachine.apply(
            ServerMessage.ServerFrameMessage(
                ServerFrameRelease(
                    roomEpoch = 1,
                    sessionEpoch = 1,
                    frame = 18,
                    canonicalFrame = 20,
                ),
            ),
        )
        stateMachine.apply(
            ServerMessage.InputFrameMessage(
                InputFrame(
                    playerIndex = 1,
                    frame = 16,
                    payload = listOf(1),
                ),
            ),
        )
        stateMachine.frameClock.markLocalFrame(31)

        val diagnostics = stateMachine.frameClock.snapshot()
        assertEquals(20, diagnostics.canonicalFrame)
        assertEquals(18, diagnostics.serverFrame)
        assertEquals(16, diagnostics.peerReadFrame)
        assertEquals(true, diagnostics.stalled)
    }

    private fun room(
        status: String,
        eventSeq: Long,
        roomEpoch: Long,
        sessionEpoch: Long,
    ) =
        NetplayJson.format.decodeFromString(
            app.shadowboy.netplay.sdk.protocol.RoomView.serializer(),
            roomJson(
                status = status,
                eventSeq = eventSeq,
                roomEpoch = roomEpoch,
                sessionEpoch = sessionEpoch,
            ),
        )
}
