package app.shadowboy.netplay.sdk.json

import app.shadowboy.netplay.sdk.protocol.ClientMessage
import app.shadowboy.netplay.sdk.protocol.ServerMessage
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

public object NetplayJson {
    public val format: Json = Json {
        classDiscriminator = "type"
        encodeDefaults = true
        explicitNulls = false
        ignoreUnknownKeys = true
    }

    public fun encodeClientMessage(message: ClientMessage): String =
        format.encodeToString<ClientMessage>(message)

    public fun decodeServerMessage(payload: String): ServerMessage =
        format.decodeFromString<ServerMessage>(payload)
}
