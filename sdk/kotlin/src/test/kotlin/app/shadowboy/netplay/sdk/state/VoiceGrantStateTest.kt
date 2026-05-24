package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.json.NetplayJson
import app.shadowboy.netplay.sdk.protocol.NetplayVoiceMode
import app.shadowboy.netplay.sdk.protocol.PlayerVoiceJoinGrant
import app.shadowboy.netplay.sdk.protocol.RoomView
import app.shadowboy.netplay.sdk.protocol.RoomVoiceStatus
import app.shadowboy.netplay.sdk.protocol.RoomVoiceView
import app.shadowboy.netplay.sdk.protocol.ServerMessage
import app.shadowboy.netplay.sdk.protocol.roomJson
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse

class VoiceGrantStateTest {
    @Test
    fun `room state keeps private voice grants token-safe in diagnostics`() {
        val stateMachine = RoomStateMachine()

        val state = stateMachine.apply(
            ServerMessage.RoomJoined(
                eventSeq = 1,
                roomEpoch = 2,
                sessionEpoch = 3,
                yourPlayerIndex = 0,
                resumeToken = "token",
                inputSocketToken = "input-token",
                voice = voiceGrant("initial-token"),
                room = roomWithVoice(eventSeq = 1, roomEpoch = 2, sessionEpoch = 3),
            ),
        )

        assertEquals("initial-token", state.voice.privateGrant?.token)
        assertEquals(true, stateMachine.diagnostics(nowMs = 1_000).voice.available)
        assertEquals("player-1", stateMachine.diagnostics(nowMs = 1_000).voice.participantIdentity)
        assertFalse(stateMachine.diagnostics(nowMs = 1_000).toString().contains("initial-token"))

        stateMachine.apply(
            ServerMessage.VoiceTokenRefreshed(
                eventSeq = 2,
                roomEpoch = 2,
                sessionEpoch = 3,
                voice = voiceGrant("fresh-token"),
            ),
        )

        assertEquals("fresh-token", stateMachine.state.voice.privateGrant?.token)
        assertEquals(2, stateMachine.state.voice.refreshedAtEventSeq)
    }

    private fun roomWithVoice(
        eventSeq: Long,
        roomEpoch: Long,
        sessionEpoch: Long,
    ): RoomView = NetplayJson.format.decodeFromString(
        RoomView.serializer(),
        roomJson(
            status = "waitingForGuest",
            eventSeq = eventSeq,
            roomEpoch = roomEpoch,
            sessionEpoch = sessionEpoch,
        ),
    ).copy(
        voice = RoomVoiceView(
            status = RoomVoiceStatus.Available,
            provider = "livekit",
            voiceRoomId = "voice-room-1",
            livekitRoomName = "sb-voice-room-1",
            serverUrl = "wss://voice.shadowboy.app",
            mode = NetplayVoiceMode.VoiceActivation,
            maxParticipants = 2,
        ),
    )

    private fun voiceGrant(token: String): PlayerVoiceJoinGrant =
        PlayerVoiceJoinGrant(
            provider = "livekit",
            voiceRoomId = "voice-room-1",
            livekitRoomName = "sb-voice-room-1",
            serverUrl = "wss://voice.shadowboy.app",
            participantIdentity = "player-1",
            token = token,
            expiresAt = "2026-05-23T21:00:00Z",
            mode = NetplayVoiceMode.VoiceActivation,
        )
}
