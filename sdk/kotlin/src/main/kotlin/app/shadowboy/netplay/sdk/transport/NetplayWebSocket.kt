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

public data class NetplayInputWebSocketJoinOptions(
    public val inviteCode: String,
    public val playerIndex: Int,
    public val roomEpoch: Long,
    public val sessionEpoch: Long,
    public val inputSocketToken: String,
)

public data class NetplayWebSocketJoinOptions(
    public val inviteCode: String,
    public val role: WebSocketRole,
    public val reconnectTicket: ReconnectTicket? = null,
    public val supportsStateFileRelay: Boolean = false,
    public val supportsRomFileRelay: Boolean = false,
)

public class NetplayWebSocketEndpoint(
    private val authHeadersProvider: NetplayAuthHeadersProvider,
) {
    public suspend fun joinRequest(
        inviteCode: String,
        role: WebSocketRole,
        reconnectTicket: ReconnectTicket? = null,
    ): NetplayWebSocketRequest =
        joinRequest(
            NetplayWebSocketJoinOptions(
                inviteCode = inviteCode,
                role = role,
                reconnectTicket = reconnectTicket,
            ),
        )

    public suspend fun joinRequest(options: NetplayWebSocketJoinOptions): NetplayWebSocketRequest {
        val path = NetplayPaths.websocketJoin(
            inviteCode = options.inviteCode,
            role = options.role.wireValue,
            reconnect = options.reconnectTicket,
            supportsStateFileRelay = options.supportsStateFileRelay,
            supportsRomFileRelay = options.supportsRomFileRelay,
        )

        return NetplayWebSocketRequest(
            pathAndQuery = path,
            headers = authHeadersProvider.headersFor(HttpMethod.Get, path, null),
        )
    }

    public suspend fun inputJoinRequest(
        options: NetplayInputWebSocketJoinOptions,
    ): NetplayWebSocketRequest {
        val path = NetplayPaths.websocketInputJoin(
            inviteCode = options.inviteCode,
            playerIndex = options.playerIndex,
            roomEpoch = options.roomEpoch,
            sessionEpoch = options.sessionEpoch,
            inputSocketToken = options.inputSocketToken,
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
