package app.shadowboy.netplay.sdk.state

import app.shadowboy.netplay.sdk.protocol.ServerMessage

public data class ReconnectTicket(
    public val playerIndex: Int,
    public val roomEpoch: Long,
    public val resumeToken: String,
)

public class ReconnectTokenStore {
    private var ticket: ReconnectTicket? = null

    public fun current(): ReconnectTicket? = ticket

    public fun clear() {
        ticket = null
    }

    public fun apply(message: ServerMessage.RoomJoined) {
        ticket = ReconnectTicket(
            playerIndex = message.yourPlayerIndex,
            roomEpoch = message.roomEpoch,
            resumeToken = message.resumeToken,
        )
    }

    public fun updateAcceptedEpoch(roomEpoch: Long) {
        ticket = ticket?.copy(roomEpoch = roomEpoch)
    }
}
