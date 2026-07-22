package app.shadowboy.netplay.sdk.protocol

import app.shadowboy.netplay.sdk.json.NetplayJson
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class ProtocolCodecTest {
    @Test
    fun `encodes heartbeat using client runtime state`() {
        val payload = NetplayJson.encodeClientMessage(
            ClientMessage.Heartbeat(
                roomEpoch = 2,
                sessionEpoch = 5,
                latestEventSeq = 9,
                localFrame = 42,
                runtimeState = ClientRuntimeState.Playing,
                network = ClientNetworkQualityReport(
                    roundTripMs = 44,
                    jitterMs = 3,
                    stallCount = 1,
                    audioRebufferEvents = 2,
                    audioMaxConsecutiveMissingFrames = 24,
                    audioQueueMinFrames = 0,
                    audioQueueMaxFrames = 8,
                ),
            ),
        )
        val json = NetplayJson.format.parseToJsonElement(payload).jsonObject

        assertEquals("heartbeat", json.string("type"))
        assertEquals("playing", json.string("runtimeState"))
        assertEquals("42", json["localFrame"]?.jsonPrimitive?.content)
        assertEquals("44", json["network"]?.jsonObject?.get("roundTripMs")?.jsonPrimitive?.content)
        assertEquals("3", json["network"]?.jsonObject?.get("jitterMs")?.jsonPrimitive?.content)
        assertEquals("1", json["network"]?.jsonObject?.get("stallCount")?.jsonPrimitive?.content)
        assertEquals("2", json["network"]?.jsonObject?.get("audioRebufferEvents")?.jsonPrimitive?.content)
        assertEquals(
            "24",
            json["network"]
                ?.jsonObject
                ?.get("audioMaxConsecutiveMissingFrames")
                ?.jsonPrimitive
                ?.content,
        )
    }

    @Test
    fun `decodes recovery resync server message`() {
        val message = NetplayJson.decodeServerMessage(
            """
            {
              "type": "recoveryResyncRequired",
              "eventSeq": 12,
              "roomEpoch": 4,
              "sessionEpoch": 7,
              "room": ${roomJson(status = "checkingCompatibility")}
            }
            """.trimIndent(),
        )

        val recovery = assertIs<ServerMessage.RecoveryResyncRequired>(message)
        assertEquals(12, recovery.eventSeq)
        assertEquals(4, recovery.room.roomEpoch)
        assertEquals(RoomStatus.CheckingCompatibility, recovery.room.status)
    }

    @Test
    fun `decodes player reconnected server message`() {
        val message = NetplayJson.decodeServerMessage(
            """
            {
              "type": "playerReconnected",
              "eventSeq": 14,
              "roomEpoch": 6,
              "sessionEpoch": 8,
              "playerIndex": 1,
              "room": ${roomJson(status = "recovering")}
            }
            """.trimIndent(),
        )

        val reconnected = assertIs<ServerMessage.PlayerReconnected>(message)
        assertEquals(1, reconnected.playerIndex)
        assertEquals(RoomStatus.Recovering, reconnected.room.status)
    }

    @Test
    fun `decodes adaptive input delay change server message`() {
        val message = NetplayJson.decodeServerMessage(
            """
            {
              "type": "inputDelayChanged",
              "eventSeq": 15,
              "roomEpoch": 6,
              "sessionEpoch": 8,
              "change": {
                "effectiveFrame": 240,
                "inputDelayFrames": 4,
                "previousInputDelayFrames": 3,
                "reason": "networkPressure"
              },
              "room": ${roomJson(status = "playing")}
            }
            """.trimIndent(),
        )

        val changed = assertIs<ServerMessage.InputDelayChanged>(message)
        assertEquals(240, changed.change.effectiveFrame)
        assertEquals(4, changed.change.inputDelayFrames)
        assertEquals(InputDelayChangeReason.NetworkPressure, changed.change.reason)
    }

    @Test
    fun `round trips explicit v5 state recovery payloads`() {
        val recoveryJson =
            """
            {
              "recoveryId": 7,
              "phase": "preparing",
              "repairFrame": 600,
              "mismatch": {
                "frame": 600,
                "repairFrame": 600,
                "hashes": [
                  {"playerIndex": 0, "sha256": "${"a".repeat(64)}"},
                  {"playerIndex": 1, "sha256": "${"b".repeat(64)}"}
                ],
                "nearbyMatches": []
              }
            }
            """.trimIndent()
        val message = NetplayJson.decodeServerMessage(
            """
            {
              "type": "stateRecoveryPrepare",
              "eventSeq": 16,
              "roomEpoch": 6,
              "sessionEpoch": 8,
              "recovery": $recoveryJson,
              "room": ${roomJson(
                status = "repairingState",
                protocolVersion = 5,
                stateRecoveryJson = recoveryJson,
              )}
            }
            """.trimIndent(),
        )

        val preparing = assertIs<ServerMessage.StateRecoveryPrepare>(message)
        assertEquals(7, preparing.recovery.recoveryId)
        assertEquals(StateRecoveryPhase.Preparing, preparing.room.stateRecovery?.phase)

        val manifest = SnapshotManifest(
            snapshotId = "recovery-7",
            repairFrame = 600,
            totalBytes = 4,
            sha256 = "c".repeat(64),
        )
        val payload = NetplayJson.encodeClientMessage(
            ClientMessage.StateRecoveryPinned(
                roomEpoch = 6,
                sessionEpoch = 8,
                pin = StateRecoveryPin(recoveryId = 7, manifest = manifest),
            ),
        )
        val json = NetplayJson.format.parseToJsonElement(payload).jsonObject
        assertEquals("stateRecoveryPinned", json.string("type"))
        assertEquals("7", json["pin"]?.jsonObject?.get("recoveryId")?.jsonPrimitive?.content)
    }

    @Test
    fun `round trips voice token refresh messages`() {
        val payload = NetplayJson.encodeClientMessage(
            ClientMessage.RefreshVoiceToken(
                roomEpoch = 4,
                sessionEpoch = 7,
            ),
        )
        val json = NetplayJson.format.parseToJsonElement(payload).jsonObject

        assertEquals("refreshVoiceToken", json.string("type"))
        assertEquals("4", json["roomEpoch"]?.jsonPrimitive?.content)

        val message = NetplayJson.decodeServerMessage(
            """
            {
              "type": "voiceTokenRefreshed",
              "eventSeq": 14,
              "roomEpoch": 4,
              "sessionEpoch": 7,
              "voice": {
                "provider": "livekit",
                "voiceRoomId": "voice-room-1",
                "livekitRoomName": "sb-voice-room-1",
                "serverUrl": "wss://livekit.shadowboy.app",
                "participantIdentity": "player-2",
                "token": "fresh-token",
                "expiresAt": "2026-05-23T21:00:00Z",
                "mode": "pushToTalk"
              }
            }
            """.trimIndent(),
        )

        val refreshed = assertIs<ServerMessage.VoiceTokenRefreshed>(message)
        assertEquals("fresh-token", refreshed.voice.token)
        assertEquals("player-2", refreshed.voice.participantIdentity)
    }

    @Test
    fun `rejects malformed rom checksums before relay calls`() {
        val session = testSessionDescriptor().copy(
            game = testSessionDescriptor().game.copy(romSha256 = "not-a-checksum"),
        )

        val error = runCatching { session.validateForRelay() }.exceptionOrNull()

        assertTrue(error is IllegalArgumentException)
    }

    @Test
    fun `encodes host client kind in room descriptors`() {
        val payload = NetplayJson.format.encodeToString(
            NetplaySessionDescriptor.serializer(),
            testSessionDescriptor().copy(hostClientKind = NetplayClientKind.Android),
        )
        val json = NetplayJson.format.parseToJsonElement(payload).jsonObject

        assertEquals("android", json.string("hostClientKind"))
    }

    @Test
    fun `round trips binary input batches`() {
        val codec = NetplayInputBatchCodec()
        val encoded = codec.encode(
            InputFrameBatch(
                roomEpoch = 2,
                sessionEpoch = 3,
                playerIndex = 1,
                frames = listOf(
                    InputFrame(playerIndex = 1, frame = 10, payload = listOf(1, 2)),
                    InputFrame(playerIndex = 1, frame = 11, payload = listOf(3, 4)),
                ),
            ),
        )

        assertEquals(
            InputFrameBatch(
                roomEpoch = 2,
                sessionEpoch = 3,
                playerIndex = 1,
                frames = listOf(
                    InputFrame(playerIndex = 1, frame = 10, payload = listOf(1, 2)),
                    InputFrame(playerIndex = 1, frame = 11, payload = listOf(3, 4)),
                ),
            ),
            codec.decode(encoded),
        )
    }

    private fun JsonObject.string(name: String): String =
        requireNotNull(this[name]).jsonPrimitive.content
}

fun testSessionDescriptor(): NetplaySessionDescriptor =
    NetplaySessionDescriptor(
        hostClientKind = NetplayClientKind.Desktop,
        hostAppVersion = "0.2.10",
        game = NetplayGameDescriptor(
            systemId = "snes",
            title = "Test Game",
            romSha256 = "a".repeat(64),
            contentKey = "test-game",
        ),
        core = NetplayCoreDescriptor(
            coreId = "snes9x",
            coreName = "Snes9x",
            coreVersion = "local",
            stateFormat = "snes9x:snes:s9x-freeze-stream-v1",
        ),
        controller = ControllerNetplayDescriptor(inputDelayFrames = 3),
    )

fun roomJson(
    status: String = "waitingForGuest",
    eventSeq: Long = 12,
    roomEpoch: Long = 4,
    sessionEpoch: Long = 7,
    protocolVersion: Int = 4,
    stateRecoveryJson: String? = null,
): String =
    """
    {
      "roomId": "00000000-0000-0000-0000-000000000001",
      "eventSeq": $eventSeq,
      "roomEpoch": $roomEpoch,
      "sessionEpoch": $sessionEpoch,
      "inviteCode": "ABCD-EF",
      "protocol": {
        "protocolVersion": $protocolVersion,
        "minSupportedProtocolVersion": 4
      },
      "session": ${NetplayJson.format.encodeToString(NetplaySessionDescriptor.serializer(), testSessionDescriptor())},
      "maxPlayers": 2,
      "pause": null,
      "stateRecovery": ${stateRecoveryJson ?: "null"},
      "status": "$status",
      "players": [
        {
          "playerIndex": 0,
          "displayNumber": 1,
          "role": "host",
          "status": "connected",
          "runtimeState": "connected",
          "occupied": true,
          "controlConnected": true,
          "inputConnected": false,
          "lastSeenAgeMs": 0,
          "reconnectGraceRemainingMs": null
        },
        {
          "playerIndex": 1,
          "displayNumber": 2,
          "role": "guest",
          "status": "empty",
          "runtimeState": "empty",
          "occupied": false,
          "controlConnected": false,
          "inputConnected": false,
          "lastSeenAgeMs": null,
          "reconnectGraceRemainingMs": null
        }
      ]
    }
    """.trimIndent()
