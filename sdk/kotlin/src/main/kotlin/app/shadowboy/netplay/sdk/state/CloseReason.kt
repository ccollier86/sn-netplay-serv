package app.shadowboy.netplay.sdk.state

public sealed class NetplayCloseReason {
    public data object Normal : NetplayCloseReason()
    public data object RoomClosed : NetplayCloseReason()
    public data object ReconnectExpired : NetplayCloseReason()
    public data object ProtocolMismatch : NetplayCloseReason()
    public data class RelayError(public val code: String, public val message: String) :
        NetplayCloseReason()
    public data class TransportClosed(public val code: Int? = null, public val reason: String? = null) :
        NetplayCloseReason()
}
