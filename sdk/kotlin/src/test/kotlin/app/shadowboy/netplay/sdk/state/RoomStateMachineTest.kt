package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.json.NetplayJson
import app.shadowboy.netplay.sdk.protocol.ClientRuntimeState
import app.shadowboy.netplay.sdk.protocol.InputFrame
import app.shadowboy.netplay.sdk.protocol.NearbyStateHashMatchView
import app.shadowboy.netplay.sdk.protocol.PlayerStateHashView
import app.shadowboy.netplay.sdk.protocol.RoomStatus
import app.shadowboy.netplay.sdk.protocol.ServerMessage
import app.shadowboy.netplay.sdk.protocol.ServerFrameRelease
import app.shadowboy.netplay.sdk.protocol.SessionPauseReason
import app.shadowboy.netplay.sdk.protocol.SessionPauseState
import app.shadowboy.netplay.sdk.protocol.SessionPauseView
import app.shadowboy.netplay.sdk.protocol.SnapshotManifest
import app.shadowboy.netplay.sdk.protocol.StateHashMismatchView
import app.shadowboy.netplay.sdk.protocol.StateRecoveryPhase
import app.shadowboy.netplay.sdk.protocol.StateRecoveryView
import app.shadowboy.netplay.sdk.protocol.roomJson
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
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
        assertEquals(NetplayResyncReason.Recovery, state.resync?.reason)
        assertEquals(NetplayResyncPhase.Requested, state.resync?.phase)
        assertEquals(NetplayResyncRole.Host, state.resync?.role)
        assertEquals(true, state.resync?.mustSendSnapshot)
        assertEquals(10, state.resync?.eventSeq)
        assertEquals(5, state.resync?.roomEpoch)
        assertEquals(9, state.resync?.sessionEpoch)
        assertEquals(true, stateMachine.resync.shouldPauseEmulation())
        assertEquals(true, stateMachine.resync.shouldClearPredictionBuffers())
        assertEquals(5, stateMachine.reconnectTokens.current()?.roomEpoch)
    }

    @Test
    fun `state hash mismatch enters resync until session restarts`() {
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

        val mismatch = StateHashMismatchView(
            frame = 120,
            repairFrame = 120,
            hashes = listOf(
                PlayerStateHashView(playerIndex = 0, sha256 = "a".repeat(64)),
                PlayerStateHashView(playerIndex = 1, sha256 = "b".repeat(64)),
            ),
            nearbyMatches = emptyList<NearbyStateHashMatchView>(),
        )
        val resyncing = stateMachine.apply(
            ServerMessage.StateHashMismatch(
                eventSeq = 11,
                roomEpoch = 2,
                sessionEpoch = 4,
                mismatch = mismatch,
                room = room(status = "checkingCompatibility", eventSeq = 11, roomEpoch = 2, sessionEpoch = 4),
            ),
        )

        assertEquals(NetplayResyncReason.StateHashMismatch, resyncing.resync?.reason)
        assertEquals(NetplayResyncPhase.Requested, resyncing.resync?.phase)
        assertEquals(mismatch, resyncing.resync?.mismatch)
        assertEquals(11, resyncing.resync?.eventSeq)

        val started = stateMachine.apply(
            ServerMessage.StartSession(
                eventSeq = 12,
                roomEpoch = 2,
                sessionEpoch = 4,
                startFrame = 0,
                room = room(status = "playing", eventSeq = 12, roomEpoch = 2, sessionEpoch = 4),
            ),
        )

        assertNull(started.resync)
    }

    @Test
    fun `v5 recovery hydrates and resets runtime only once per recovery id`() {
        val stateMachine = RoomStateMachine()
        val preparing = recovery(StateRecoveryPhase.Preparing)
        val preparingRoom = room(
            status = "repairingState",
            eventSeq = 1,
            roomEpoch = 2,
            sessionEpoch = 3,
        ).copy(
            protocol = room(
                status = "repairingState",
                eventSeq = 1,
                roomEpoch = 2,
                sessionEpoch = 3,
            ).protocol.copy(protocolVersion = 5),
            stateRecovery = preparing,
        )

        val hydrated = stateMachine.apply(
            ServerMessage.RoomJoined(
                eventSeq = 1,
                roomEpoch = 2,
                sessionEpoch = 3,
                yourPlayerIndex = 0,
                resumeToken = "token",
                inputSocketToken = "input-token",
                room = preparingRoom,
            ),
        )

        assertEquals(preparing, hydrated.stateRecovery)
        assertNull(hydrated.resync)
        assertTrue(hydrated.runtimeResetRequired)
        assertEquals(NetplayEffectivePauseReason.StateResync, stateMachine.effectivePauseReason())

        stateMachine.acknowledgeRuntimeReset()
        stateMachine.apply(
            ServerMessage.StateRecoveryPrepare(
                eventSeq = 2,
                roomEpoch = 2,
                sessionEpoch = 3,
                recovery = preparing,
                room = preparingRoom.copy(eventSeq = 2),
            ),
        )
        assertFalse(stateMachine.state.runtimeResetRequired)

        val manifest = SnapshotManifest(
            snapshotId = "recovery-7",
            repairFrame = 600,
            totalBytes = 4,
            sha256 = "c".repeat(64),
        )
        val committed = recovery(StateRecoveryPhase.Committed, manifest)
        val committedRoom = preparingRoom.copy(
            eventSeq = 3,
            sessionEpoch = 4,
            status = RoomStatus.CheckingCompatibility,
            stateRecovery = committed,
        )
        stateMachine.apply(
            ServerMessage.StateRecoveryCommitted(
                eventSeq = 3,
                roomEpoch = 2,
                sessionEpoch = 4,
                recovery = committed,
                room = committedRoom,
            ),
        )

        assertEquals(committed, stateMachine.state.stateRecovery)
        assertFalse(stateMachine.state.runtimeResetRequired)

        stateMachine.apply(
            ServerMessage.StartSession(
                eventSeq = 4,
                roomEpoch = 2,
                sessionEpoch = 4,
                startFrame = 600,
                room = committedRoom.copy(eventSeq = 4, status = RoomStatus.StartScheduled),
            ),
        )
        assertEquals(committed, stateMachine.state.stateRecovery)

        stateMachine.markStateRecoveryFrameExecuted(2, 4, 601)
        assertEquals(committed, stateMachine.state.stateRecovery)
        stateMachine.markStateRecoveryFrameExecuted(2, 4, 600)
        assertNull(stateMachine.state.stateRecovery)
        assertNull(stateMachine.state.room?.stateRecovery)
    }

    @Test
    fun `resync coordinator exposes snapshot decisions`() {
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
            ServerMessage.StateHashMismatch(
                eventSeq = 11,
                roomEpoch = 2,
                sessionEpoch = 4,
                mismatch = StateHashMismatchView(
                    frame = 120,
                    repairFrame = 120,
                    hashes = listOf(
                        PlayerStateHashView(playerIndex = 0, sha256 = "a".repeat(64)),
                        PlayerStateHashView(playerIndex = 1, sha256 = "b".repeat(64)),
                    ),
                    nearbyMatches = emptyList<NearbyStateHashMatchView>(),
                ),
                room = room(status = "checkingCompatibility", eventSeq = 11, roomEpoch = 2, sessionEpoch = 4),
            ),
        )

        stateMachine.resync.markSnapshotNeeded(nowMs = 1_000)

        assertEquals(true, stateMachine.resync.shouldSendHostSnapshot())
        assertEquals(false, stateMachine.resync.shouldWaitForSnapshot())
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
    fun `heartbeat can consume runtime telemetry safely`() {
        val tracker = HeartbeatTracker()
        val telemetry = RuntimeTelemetryTracker()

        telemetry.markLocalFrame(90)
        telemetry.recordRoundTrip(40)
        telemetry.recordRoundTrip(48)
        telemetry.recordStall()
        telemetry.recordCatchUpFrames(2)

        val heartbeat = tracker.heartbeatMessage(
            roomEpoch = 2,
            sessionEpoch = 3,
            latestEventSeq = 4,
            localFrame = null,
            runtimeState = ClientRuntimeState.Playing,
            telemetry = telemetry,
        )

        assertEquals(90, heartbeat.localFrame)
        assertEquals(48, heartbeat.network?.roundTripMs)
        assertEquals(1, heartbeat.network?.stallCount)
        assertEquals(2, heartbeat.network?.catchUpFrames)
        assertEquals(0, telemetry.snapshot().network.stallCount)
    }

    @Test
    fun `state hash reporter normalizes hashes and deduplicates frames`() {
        val reporter = StateHashReporter(StateHashReporterPolicy(reportEveryFrames = 30))

        assertEquals(true, reporter.shouldReport(0))
        reporter.stateHashMessage(
            roomEpoch = 2,
            sessionEpoch = 3,
            frame = 0,
            sha256 = "a".repeat(64),
        )
        assertEquals(false, reporter.shouldReport(29))
        assertEquals(true, reporter.shouldReport(30))
        assertEquals(
            "a".repeat(64),
            reporter.stateHashMessage(
                roomEpoch = 2,
                sessionEpoch = 3,
                frame = 30,
                sha256 = "A".repeat(64),
            ).report.sha256,
        )
        assertEquals(false, reporter.shouldReport(30))
        assertEquals(false, reporter.shouldReport(59))
        assertEquals(true, reporter.shouldReport(61))
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
                roomEpoch = 1,
                sessionEpoch = 1,
                input = InputFrame(
                    playerIndex = 1,
                    frame = 16,
                    payload = listOf(1),
                ),
            ),
        )
        stateMachine.frameClock.markLocalFrame(80)

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

    private fun recovery(
        phase: StateRecoveryPhase,
        manifest: SnapshotManifest? = null,
    ) =
        StateRecoveryView(
            recoveryId = 7,
            phase = phase,
            repairFrame = 600,
            mismatch = StateHashMismatchView(
                frame = 600,
                repairFrame = 600,
                hashes = listOf(
                    PlayerStateHashView(playerIndex = 0, sha256 = "a".repeat(64)),
                    PlayerStateHashView(playerIndex = 1, sha256 = "b".repeat(64)),
                ),
            ),
            pinnedSnapshot = manifest,
        )

}
