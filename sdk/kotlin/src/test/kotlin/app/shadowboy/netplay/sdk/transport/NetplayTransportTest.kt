package app.shadowboy.netplay.sdk.transport

import app.shadowboy.netplay.sdk.NetplayPaths
import app.shadowboy.netplay.sdk.json.NetplayJson
import app.shadowboy.netplay.sdk.protocol.CreateRoomRequest
import app.shadowboy.netplay.sdk.protocol.CreateRoomResponse
import app.shadowboy.netplay.sdk.protocol.roomJson
import app.shadowboy.netplay.sdk.protocol.testSessionDescriptor
import app.shadowboy.netplay.sdk.state.ReconnectTicket
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.encodeToString

class NetplayTransportTest {
    @Test
    fun `rest client signs and sends create room request`() = runTest {
        val transport = CapturingTransport(
            NetplayJson.format.encodeToString(
                CreateRoomResponse.serializer(),
                CreateRoomResponse(
                    room = NetplayJson.format.decodeFromString(
                        app.shadowboy.netplay.sdk.protocol.RoomView.serializer(),
                        roomJson(),
                    ),
                ),
            ),
        )
        val auth = CapturingAuthHeadersProvider()
        val client = NetplayRestClient(transport, auth)

        client.createRoom(CreateRoomRequest(session = testSessionDescriptor()))

        val request = assertNotNull(transport.lastRequest)
        assertEquals(HttpMethod.Post, request.method)
        assertEquals("/v1/rooms", request.pathAndQuery)
        assertEquals("signed", request.headers["Authorization"])
        assertTrue(requireNotNull(request.body).contains("\"desktopProtocolVersion\":4"))
        assertEquals(request.pathAndQuery, auth.lastPath)
        assertEquals(request.body, auth.lastBody)
    }

    @Test
    fun `websocket reconnect path includes escaped token and epoch`() = runTest {
        val auth = CapturingAuthHeadersProvider()
        val endpoint = NetplayWebSocketEndpoint(auth)

        val request = endpoint.joinRequest(
            inviteCode = "AB CD",
            role = WebSocketRole.Host,
            reconnectTicket = ReconnectTicket(
                playerIndex = 0,
                roomEpoch = 4,
                resumeToken = "resume token/+",
            ),
        )

        assertEquals(
            "/v1/ws?inviteCode=AB%20CD&protocolVersion=4&supportsStateFileRelay=false&supportsRomFileRelay=false&playerIndex=0&roomEpoch=4&resumeToken=resume%20token%2F%2B",
            request.pathAndQuery,
        )
        assertEquals(request.pathAndQuery, auth.lastPath)
    }

    @Test
    fun `input websocket path includes token and session epoch`() = runTest {
        val auth = CapturingAuthHeadersProvider()
        val endpoint = NetplayWebSocketEndpoint(auth)

        val request = endpoint.inputJoinRequest(
            NetplayInputWebSocketJoinOptions(
                inviteCode = "AB CD",
                playerIndex = 1,
                roomEpoch = 4,
                sessionEpoch = 7,
                inputSocketToken = "input token/+",
            ),
        )

        assertEquals(
            "/v1/ws/input?inviteCode=AB%20CD&protocolVersion=4&playerIndex=1&roomEpoch=4&sessionEpoch=7&inputSocketToken=input%20token%2F%2B",
            request.pathAndQuery,
        )
        assertEquals(request.pathAndQuery, auth.lastPath)
    }

    @Test
    fun `room status path escapes invite code`() {
        assertEquals("/v1/rooms/AB%20CD/status", NetplayPaths.roomStatus("AB CD"))
    }
}

private class CapturingTransport(private val responseBody: String) : NetplayHttpTransport {
    var lastRequest: NetplayHttpRequest? = null

    override suspend fun execute(request: NetplayHttpRequest): NetplayHttpResponse {
        lastRequest = request
        return NetplayHttpResponse(statusCode = 200, body = responseBody)
    }
}

private class CapturingAuthHeadersProvider : NetplayAuthHeadersProvider {
    var lastPath: String? = null
    var lastBody: String? = null

    override suspend fun headersFor(
        method: HttpMethod,
        pathAndQuery: String,
        body: String?,
    ): Map<String, String> {
        lastPath = pathAndQuery
        lastBody = body
        return mapOf("Authorization" to "signed")
    }
}
