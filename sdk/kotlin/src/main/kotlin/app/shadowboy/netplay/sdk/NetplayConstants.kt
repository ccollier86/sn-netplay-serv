package app.shadowboy.netplay.sdk

import app.shadowboy.netplay.sdk.state.ReconnectTicket
import java.net.URLEncoder

public const val NETPLAY_PROTOCOL_VERSION: Int = 3
public const val MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION: Int = 3

public object NetplayPaths {
    public const val CREATE_ROOM: String = "/v1/rooms"

    public fun roomStatus(inviteCode: String): String =
        "/v1/rooms/${encode(inviteCode.trim())}/status"

    public fun websocketJoin(
        inviteCode: String,
        role: String,
        reconnect: ReconnectTicket? = null,
    ): String {
        val base = "/v1/ws?inviteCode=${encode(inviteCode.trim())}"
        val protocol = "&protocolVersion=$NETPLAY_PROTOCOL_VERSION"
        if (reconnect == null) {
            return "$base&role=${encode(role)}$protocol"
        }

        return "$base$protocol" +
            "&playerIndex=${reconnect.playerIndex}" +
            "&roomEpoch=${reconnect.roomEpoch}" +
            "&resumeToken=${encode(reconnect.resumeToken)}"
    }

    private fun encode(value: String): String =
        URLEncoder.encode(value, Charsets.UTF_8.name()).replace("+", "%20")
}
