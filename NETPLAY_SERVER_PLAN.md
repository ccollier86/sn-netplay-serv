# ShadowBoy Netplay Server Plan

## Goal

Build a small Rust relay server that lets two ShadowBoy Desktop players connect
with an invite code and play locally synchronized netplay. The server never runs
emulators, never receives ROMs, and never streams gameplay.

Implementation must follow `NETPLAY_ENGINEERING_RULES.md`: SOLID principles,
strict separation of concern, no god files, 400-500 non-comment lines maximum
per source file, module front matter, documented public method signatures, and
tests added as each system is built.

## Product Flow

1. Host opens a game and clicks `Play Together`.
2. Desktop sends its protected-client access token, install id, and signed
   request proof to the netplay server.
3. Desktop includes protocol version plus a sanitized game/core descriptor.
4. Netplay server creates a short invite code.
5. Guest enters the invite code in ShadowBoy Desktop.
6. Desktop previews the room descriptor and matches a local ROM by hash.
7. Server validates the guest license the same way.
8. Desktop clients exchange compatibility fingerprints.
9. For controller netplay, host sends a save-state snapshot to the guest through
   the relay.
10. For link-cable mode, clients exchange link compatibility and then relay
   virtual cable packets after both are ready.
11. Both clients start from the same server-assigned player slots.
12. Server relays frame-numbered input messages or link-cable packets until the
   session ends.

## Rust Stack

- `tokio` for async runtime.
- `axum` for HTTP and WebSocket endpoints.
- `serde` / `serde_json` for the MVP protocol.
- `reqwest` for calling the existing ShadowBoy license authority.
- `tracing` / `tracing-subscriber` for logs.
- `uuid` or `ulid` for internal ids.
- `dashmap` or `tokio::sync::RwLock<HashMap<...>>` for in-memory rooms.

## Server Modules

```text
src/
  main.rs
  config.rs
  auth/
    license_authority_client.rs
    verified_license.rs
  rooms/
    invite_code.rs
    room.rs
    room_registry.rs
  protocol/
    client_message.rs
    server_message.rs
    compatibility.rs
    input_frame.rs
    link_cable_compatibility.rs
    link_cable_descriptor.rs
    link_cable_packet.rs
    snapshot.rs
  transport/
    websocket_session.rs
  limits.rs
```

## HTTP Endpoints

```text
GET  /health
POST /v1/rooms
GET  /v1/rooms/:inviteCode/status
GET  /v1/ws?inviteCode=XXXX&protocolVersion=1
```

`POST /v1/rooms` requires a valid ShadowBoy protected-client token and returns
an invite code. The request body contains the netplay protocol version and
game/core descriptor used for invite preview and local ROM matching. The
WebSocket join also requires authorization.

The relay must never transfer ROM files. It only stores hashes and stable ids so
ShadowBoy can tell the guest whether they already have the correct local
content.

Rooms have an explicit `mode`:

- `controllerNetplay` keeps the current same-ROM, save-state-sync, lockstep
  input flow.
- `linkCable` is for independent emulator instances connected by a virtual link
  cable. It uses a platform-neutral `link` descriptor and does not require ROM
  hashes to match.

## License Validation

The netplay server should not duplicate billing/license logic. It calls the
existing metadata/cheat service using a server-to-server secret and asks that
trusted service to authorize the Desktop install/session for feature `netplay`.

```text
POST /internal/netplay/authorize
Authorization: Bearer <netplay-internal-secret>
```

Request:

```json
{
  "clientKind": "desktop",
  "accessToken": "<client-access-token>",
  "installationId": "<installation-id>",
  "feature": "netplay",
  "requiredEntitlement": "premiumOrTrial",
  "protectedRequest": {
    "method": "POST",
    "pathAndQuery": "/v1/rooms",
    "bodySha256Hex": "<sha256>",
    "nonce": "<X-Req-Nonce or null>",
    "signature": "<X-Req-Sig or null>",
    "timestamp": "<X-Req-Ts or null>"
  }
}
```

Response:

```json
{
  "authorized": true,
  "subjectId": "license-or-install-id",
  "tier": "premium",
  "features": {
    "netplay": true
  },
  "expiresAt": "2026-05-17T18:30:00Z"
}
```

Cache valid responses briefly, about 60 seconds. Cache invalid responses briefly,
about 10 seconds. Never log tokens.

## Room Rules

- Two players only for MVP.
- Model rooms as capacity-limited player slots, not hardcoded two-player fields.
- MVP room capacity is `2`.
- Host takes Player 1 by default.
- First guest takes Player 2 by default.
- The server is authoritative for player slot assignment.
- Clients must not choose or spoof their own player index.
- Short invite code, for example `8K4X-2Q`.
- Room expires if no guest joins within 10 minutes.
- Room closes when host leaves.

## Link Cable Rules

- First release supports two-player GBA link rooms.
- The server still never receives ROMs and never runs emulators.
- Host is cable Player 1; guest is cable Player 2.
- `runtimeProfile` describes compatible client runtimes, not operating systems.
- Desktop and Android can join only when `linkProtocol`, `runtimeProfile`, and
  required system-data hashes match.
- Link packets are opaque bytes to the relay.
- Link packet sequence must increase per player.
- Link packets are relayed to the other player and not echoed to the sender.
- No long-term persistence needed for MVP.

## Player Slots And Status

The MVP room exposes two player slots, but the data model should support more
slots later without changing the protocol shape.

```text
Player 1: host
Player 2: guest
```

Each slot should expose a status that Desktop can render directly:

```text
empty
connecting
connected
checkingCompatibility
compatibilityFailed
syncingState
ready
playing
disconnected
```

Server room state should include:

```rust
struct NetplayRoom {
    room_id: RoomId,
    invite_code: InviteCode,
    max_players: u8,             // 2 for MVP
    players: Vec<PlayerSlot>,    // length is max_players
    status: RoomStatus,
}

struct PlayerSlot {
    slot: PlayerSlotId,          // zero-based internally, displayed as Player 1+
    player_index: u8,            // 0 for Player 1, 1 for Player 2
    role: PlayerRole,            // Host or Guest
    subject_id: Option<String>,  // verified license/install/account id
    connection_id: Option<ConnectionId>,
    display_name: Option<String>,
    status: PlayerStatus,
    last_seen_at: Option<Instant>,
    average_ping_ms: Option<u32>,
}
```

When a client joins, the server sends:

```json
{
  "type": "roomJoined",
  "roomId": "room-id",
  "inviteCode": "8K4X-2Q",
  "yourSlot": "player1",
  "yourRole": "host",
  "players": [
    { "slot": "player1", "role": "host", "status": "connected" },
    { "slot": "player2", "role": "guest", "status": "empty" }
  ]
}
```

Every status change should be broadcast as a room update:

```json
{
  "type": "roomStateChanged",
  "players": [
    { "slot": "player1", "role": "host", "status": "ready", "pingMs": 24 },
    { "slot": "player2", "role": "guest", "status": "syncingState", "pingMs": 41 }
  ]
}
```

Input relay validation:

- Host may only send input for Player 1.
- Guest may only send input for Player 2.
- Server rejects any `inputFrame` whose `playerIndex` does not match the
  connection's assigned slot.
- Server relays the assigned slot with each input frame so clients never infer
  identity from connection order.
- Protocol messages should always use `playerIndex` plus a `players` array, not
  special `hostInput` / `guestInput` fields.
- Later expansion to 3 or 4 players should only require increasing room
  capacity, adding UI slots, and allowing more joined player connections.

MVP does not need slot swapping. Add it later as an explicit host-controlled room
action if needed.

## WebSocket Messages

Client to server:

```text
joinRoom
setCompatibilityFingerprint
ready
snapshotChunk
snapshotComplete
inputFrame
syncHash
resyncRequest
pause
resume
leave
pong
```

Server to client:

```text
roomJoined
roomStateChanged
playerJoined
playerLeft
compatibilityFingerprint
compatibilityAccepted
compatibilityRejected
snapshotChunk
snapshotComplete
startSession
inputFrame
syncHash
desyncDetected
resyncRequested
sessionPaused
sessionResumed
error
ping
```

## Compatibility Fingerprint

Host and guest must match before gameplay starts.

```json
{
  "platformId": "n64",
  "coreId": "mupen64plus-next",
  "coreVersion": "string",
  "romHash": "sha256",
  "settingsHash": "sha256",
  "saveStateVersion": "string",
  "desktopProtocolVersion": 1
}
```

## Relay Rules

- Save-state chunks are relayed, not stored permanently.
- Input frames are relayed by frame number.
- Server validates room membership, player slot, message size, and rate.
- Server does not interpret game logic.
- Server does not accept ROM uploads.

## Limits

- Max rooms per license/install.
- Max joins per IP per minute.
- Max WebSocket message size.
- Max save-state snapshot size.
- Max buffered input frames.
- Idle room timeout.
- Hard room lifetime timeout.

## MVP Build Order

1. Server skeleton with config, health endpoint, and tracing.
2. License authority client.
3. Invite-code room creation and in-memory room registry.
4. WebSocket join with host/guest slots.
5. Compatibility fingerprint exchange.
6. Save-state chunk relay.
7. Frame-numbered input relay.
8. Ping/latency and disconnect handling.
9. Rate limits and message size limits.
10. Desktop integration.
