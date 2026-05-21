package app.shadowboy.netplay.sdk.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
public enum class NetplayClientKind {
    @SerialName("desktop")
    Desktop,

    @SerialName("android")
    Android,
}

@Serializable
public enum class NetplaySessionMode {
    @SerialName("controllerNetplay")
    ControllerNetplay,

    @SerialName("linkCable")
    LinkCable,
}

@Serializable
public enum class LinkCableTransport {
    @SerialName("relay")
    Relay,
}

@Serializable
public enum class PlayerRole {
    @SerialName("host")
    Host,

    @SerialName("guest")
    Guest,
}

@Serializable
public enum class PlayerStatus {
    @SerialName("empty")
    Empty,

    @SerialName("connected")
    Connected,

    @SerialName("checkingCompatibility")
    CheckingCompatibility,

    @SerialName("compatibilityFailed")
    CompatibilityFailed,

    @SerialName("syncingState")
    SyncingState,

    @SerialName("ready")
    Ready,

    @SerialName("playing")
    Playing,

    @SerialName("paused")
    Paused,

    @SerialName("reconnecting")
    Reconnecting,

    @SerialName("recoveryExpired")
    RecoveryExpired,

    @SerialName("disconnected")
    Disconnected,
}

@Serializable
public enum class PlayerRuntimeState {
    @SerialName("empty")
    Empty,

    @SerialName("connected")
    Connected,

    @SerialName("checkingCompatibility")
    CheckingCompatibility,

    @SerialName("syncing")
    Syncing,

    @SerialName("ready")
    Ready,

    @SerialName("playing")
    Playing,

    @SerialName("pausing")
    Pausing,

    @SerialName("paused")
    Paused,

    @SerialName("reconnecting")
    Reconnecting,

    @SerialName("stale")
    Stale,

    @SerialName("disconnected")
    Disconnected,

    @SerialName("recoveryExpired")
    RecoveryExpired,
}

@Serializable
public enum class ClientRuntimeState {
    @SerialName("connected")
    Connected,

    @SerialName("checkingCompatibility")
    CheckingCompatibility,

    @SerialName("syncing")
    Syncing,

    @SerialName("ready")
    Ready,

    @SerialName("playing")
    Playing,

    @SerialName("pausing")
    Pausing,

    @SerialName("paused")
    Paused,

    @SerialName("reconnecting")
    Reconnecting,

    @SerialName("disconnected")
    Disconnected,
}

@Serializable
public enum class RoomStatus {
    @SerialName("waitingForGuest")
    WaitingForGuest,

    @SerialName("checkingCompatibility")
    CheckingCompatibility,

    @SerialName("syncingState")
    SyncingState,

    @SerialName("ready")
    Ready,

    @SerialName("playing")
    Playing,

    @SerialName("paused")
    Paused,

    @SerialName("recovering")
    Recovering,

    @SerialName("closed")
    Closed,
}

@Serializable
public enum class SessionPauseReason {
    @SerialName("menu")
    Menu,

    @SerialName("backgrounded")
    Backgrounded,

    @SerialName("system")
    System,

    @SerialName("connectionLost")
    ConnectionLost,
}

@Serializable
public enum class SessionPauseState {
    @SerialName("pausing")
    Pausing,

    @SerialName("paused")
    Paused,
}
