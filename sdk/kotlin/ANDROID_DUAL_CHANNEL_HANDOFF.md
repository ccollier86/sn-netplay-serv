# Android Netplay Dual-Channel Handoff

## SDK Status

The Kotlin SDK has been updated for relay protocol v4 and verified locally:

```bash
JAVA_HOME=/home/catalyst-2/.local/jdk-21 /home/catalyst-2/projects/gba-emulator/gradlew -p /home/catalyst-2/projects/sb-desktop/sb-netplay-serv/sdk/kotlin test
```

## Protocol Version

- `NETPLAY_PROTOCOL_VERSION = 4`
- `MIN_SUPPORTED_NETPLAY_PROTOCOL_VERSION = 4`
- Protocol v3 clients are intentionally rejected by the relay after this change.

## Control Socket

The existing JSON room socket remains:

```text
GET /v1/ws?inviteCode=<code>&role=<host|guest>&protocolVersion=4
```

Reconnect still uses:

```text
GET /v1/ws?inviteCode=<code>&protocolVersion=4&playerIndex=<slot>&roomEpoch=<epoch>&resumeToken=<token>
```

`roomJoined` now includes:

```json
{
  "type": "roomJoined",
  "eventSeq": 1,
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "yourPlayerIndex": 0,
  "resumeToken": "opaque-reconnect-token",
  "inputSocketToken": "opaque-input-socket-token",
  "room": {}
}
```

Android should store `resumeToken` for reconnect and immediately use `inputSocketToken` to open the binary input socket.

## Binary Input Socket

New high-frequency input socket:

```text
GET /v1/ws/input?inviteCode=<code>&protocolVersion=4&playerIndex=<slot>&roomEpoch=<roomEpoch>&sessionEpoch=<sessionEpoch>&inputSocketToken=<token>
```

Authentication is the same protected-client signing flow as the control WebSocket:

- Method: `GET`
- Body hash: SHA256 empty body
- Path and query must exactly match the input socket request path.

The relay will not let controller-netplay rooms start until both players have connected their control socket and input socket.

## Binary Batch Format

The Kotlin SDK owns this codec in:

```text
sdk/kotlin/src/main/kotlin/app/shadowboy/netplay/sdk/protocol/InputBatch.kt
```

Use `NetplayInputBatchCodec.encode(...)` for local input and `decode(...)` for remote input.

Wire format:

```text
magic: "SBI1"
messageType: u8 = 1
roomEpoch: u64 big-endian
sessionEpoch: u64 big-endian
playerIndex: u8
frameCount: u8, max 4
repeated:
  frame: u64 big-endian
  payloadLength: u16 big-endian
  payload bytes
```

Rules:

- Input socket only accepts binary batches.
- Control socket should no longer send `inputFrame` JSON for normal controller netplay.
- Relay fans out binary input batches only to the other player’s input socket.
- If the input socket closes during active gameplay, treat it like a connection disruption and enter recovery/reconnect.

## Client Flow

Host:

1. Create room with `desktopProtocolVersion`/protocol version `4`.
2. Open control socket as host.
3. Receive `roomJoined`.
4. Open input socket using `inputSocketToken`, `yourPlayerIndex`, `roomEpoch`, and `sessionEpoch`.
5. Send compatibility on the control socket.
6. Send host snapshot over the control socket.
7. Send `ready` only after snapshot complete and input socket is connected.
8. Send gameplay input over the input socket as binary batches.

Guest:

1. Preview/get room status and verify ROM/core/state compatibility.
2. Open control socket as guest.
3. Receive `roomJoined`.
4. Open input socket using `inputSocketToken`, `yourPlayerIndex`, `roomEpoch`, and `sessionEpoch`.
5. Send compatibility on the control socket.
6. Receive/load host snapshot.
7. Send `ready` only after snapshot load and input socket is connected.
8. Send gameplay input over the input socket as binary batches.

## Room View Additions

`NetplaySessionDescriptor` now includes the app family that created the room:

```kotlin
val hostClientKind: NetplayClientKind?
```

The relay stamps this from authenticated request headers when the room is
created. Clients should not trust their own request body for this value.
Android-created rooms will come back as `NetplayClientKind.Android`; Desktop
rooms will come back as `NetplayClientKind.Desktop`.

`PlayerSlotView` now includes:

```kotlin
val controlConnected: Boolean
val inputConnected: Boolean
```

Android can show these in debug UI, but production UI does not need to expose them.

## Input Delay

Manual input-delay tuning should be removed from client settings.

Current transition contract:

- Room descriptor still has `controller.inputDelayFrames`, default `3`, for compatibility.
- Clients should treat this as automatic room timing, not a user setting.
- Long term, relay-owned/adaptive delay can update the room contract; Android should avoid hard-coding local user delay into deterministic compatibility hashes.

## Important Runtime Notes

- Use the room’s `controller.inputDelayFrames` when starting lockstep until adaptive relay delay is added.
- Do not include local input delay in compatibility hashes.
- Keep `settingsHash = SHA256("")` unless Android and desktop share deterministic emulator options.
- Keep `cheatsHash = SHA256("")` when no synced cheats are active.
- Keep `systemDataHash = null` for NES/SNES/Genesis/SMS unless a BIOS/system file is actually required.

## Kotlin SDK Files Changed

- `NetplayConstants.kt`: protocol v4 and input socket path builder.
- `protocol/ProtocolEnums.kt`: `NetplayClientKind`.
- `protocol/SessionDescriptors.kt`: `hostClientKind`.
- `transport/NetplayWebSocket.kt`: `inputJoinRequest(...)`.
- `protocol/Messages.kt`: `roomJoined.inputSocketToken`.
- `protocol/RoomViews.kt`: `controlConnected`, `inputConnected`.
- `protocol/InputBatch.kt`: binary input batch codec.
- Tests cover room message fields, path builders, and codec round-trip.
