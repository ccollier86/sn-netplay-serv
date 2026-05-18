package app.shadowboy.netplay.sdk.transport

import app.shadowboy.netplay.sdk.NetplayPaths
import app.shadowboy.netplay.sdk.json.NetplayJson
import app.shadowboy.netplay.sdk.protocol.ClientMessage
import app.shadowboy.netplay.sdk.protocol.ServerMessage
import app.shadowboy.netplay.sdk.state.ReconnectTicket

public enum class WebSocketRole(public val wireValue: String) {
    Host("host"),
    Guest("guest"),
}

public data class NetplayWebSocketRequest(
    public val pathAndQuery: String,
    public val headers: Map<String, String>,
)

public class NetplayWebSocketEndpoint(
    private val authHeadersProvider: NetplayAuthHeadersProvider,
) {
    public suspend fun joinRequest(
        inviteCode: String,
        role: WebSocketRole,
        reconnectTicket: ReconnectTicket? = null,
    ): NetplayWebSocketRequest {
        val path = NetplayPaths.websocketJoin(
            inviteCode = inviteCode,
            role = role.wireValue,
            reconnect = reconnectTicket,
        )

        return NetplayWebSocketRequest(
            pathAndQuery = path,
            headers = authHeadersProvider.headersFor(HttpMethod.Get, path, null),
        )
    }
}

public class NetplayMessageCodec {
    public fun encode(message: ClientMessage): String =
        NetplayJson.encodeClientMessage(message)

    public fun decode(payload: String): ServerMessage =
        NetplayJson.decodeServerMessage(payload)
}
