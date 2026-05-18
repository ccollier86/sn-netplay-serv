package app.shadowboy.netplay.sdk.transport

import app.shadowboy.netplay.sdk.NetplayPaths
import app.shadowboy.netplay.sdk.json.NetplayJson
import app.shadowboy.netplay.sdk.protocol.CreateRoomRequest
import app.shadowboy.netplay.sdk.protocol.CreateRoomResponse
import app.shadowboy.netplay.sdk.protocol.RoomStatusResponse
import app.shadowboy.netplay.sdk.protocol.validateForRelay
import kotlinx.serialization.encodeToString

public enum class HttpMethod {
    Get,
    Post,
}

public data class NetplayHttpRequest(
    public val method: HttpMethod,
    public val pathAndQuery: String,
    public val headers: Map<String, String>,
    public val body: String? = null,
)

public data class NetplayHttpResponse(
    public val statusCode: Int,
    public val body: String,
)

public interface NetplayHttpTransport {
    public suspend fun execute(request: NetplayHttpRequest): NetplayHttpResponse
}

public interface NetplayAuthHeadersProvider {
    public suspend fun headersFor(
        method: HttpMethod,
        pathAndQuery: String,
        body: String?,
    ): Map<String, String>
}

public class NetplayRestClient(
    private val transport: NetplayHttpTransport,
    private val authHeadersProvider: NetplayAuthHeadersProvider,
) {
    public suspend fun createRoom(request: CreateRoomRequest): CreateRoomResponse {
        request.session.validateForRelay()
        val body = NetplayJson.format.encodeToString(request)
        val path = NetplayPaths.CREATE_ROOM
        val response = transport.execute(
            NetplayHttpRequest(
                method = HttpMethod.Post,
                pathAndQuery = path,
                headers = authHeadersProvider.headersFor(HttpMethod.Post, path, body),
                body = body,
            ),
        )

        return response.decodeSuccessful()
    }

    public suspend fun roomStatus(inviteCode: String): RoomStatusResponse {
        val path = NetplayPaths.roomStatus(inviteCode)
        val response = transport.execute(
            NetplayHttpRequest(
                method = HttpMethod.Get,
                pathAndQuery = path,
                headers = authHeadersProvider.headersFor(HttpMethod.Get, path, null),
            ),
        )

        return response.decodeSuccessful()
    }

    private inline fun <reified T> NetplayHttpResponse.decodeSuccessful(): T {
        if (statusCode !in 200..299) {
            throw NetplayRestException(statusCode, body)
        }
        return NetplayJson.format.decodeFromString<T>(body)
    }
}

public class NetplayRestException(
    public val statusCode: Int,
    public val responseBody: String,
) : RuntimeException("Netplay REST request failed with HTTP $statusCode")
