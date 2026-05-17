# Netplay MVP Protocol

## HTTP

### `GET /health`

Returns process health.

```json
{
  "status": "ok"
}
```

### `POST /v1/rooms`

Creates a room for the verified host.

Headers:

```text
Authorization: Bearer <desktop-token>
X-Install-Id: <installationId>
X-Req-Ts: <epoch milliseconds>
X-Req-Nonce: <unique nonce>
X-Req-Sig: <base64 signature>
```

Desktop should create the request signature using the same protected-client
signer it already uses for `/v1/desktop/*` metadata, cheat, billing, and update
requests.

The relay asks the metadata service to authorize feature `netplay`. The metadata
service remains the authority for premium or active-trial entitlement.

Response:

```json
{
  "room": {
    "roomId": "<uuid>",
    "inviteCode": "AB23-CD",
    "maxPlayers": 2,
    "status": "waitingForGuest",
    "players": [
      {
        "playerIndex": 0,
        "displayNumber": 1,
        "role": "host",
        "status": "connected",
        "occupied": true
      },
      {
        "playerIndex": 1,
        "displayNumber": 2,
        "role": "guest",
        "status": "empty",
        "occupied": false
      }
    ]
  }
}
```

### `GET /v1/rooms/{invite_code}/status`

Returns the current room view for a user-entered invite code.

## WebSocket

### `GET /v1/ws?inviteCode=AB23-CD&role=host`

Attaches the room creator's Desktop client to Player 1.

Required headers match room creation:

```text
Authorization: Bearer <desktop-token>
X-Install-Id: <installationId>
X-Req-Ts: <epoch milliseconds>
X-Req-Nonce: <unique nonce>
X-Req-Sig: <base64 signature>
```

### `GET /v1/ws?inviteCode=AB23-CD&role=guest`

Adds the guest Desktop client to Player 2.

`role` defaults to `guest` when omitted.

First successful socket message:

```json
{
  "type": "roomJoined",
  "yourPlayerIndex": 1,
  "room": {
    "roomId": "<uuid>",
    "inviteCode": "AB23-CD",
    "maxPlayers": 2,
    "status": "checkingCompatibility",
    "players": []
  }
}
```

Whenever room state changes, subscribed sockets receive:

```json
{
  "type": "roomStateChanged",
  "room": {
    "roomId": "<uuid>",
    "inviteCode": "AB23-CD",
    "maxPlayers": 2,
    "status": "checkingCompatibility",
    "players": []
  }
}
```

The socket handles:

```json
{ "type": "ping" }
```

and replies:

```json
{ "type": "pong" }
```

## Compatibility Fingerprint

Desktop sends these fields before the session can start:

```json
{
  "type": "setCompatibilityFingerprint",
  "fingerprint": {
    "desktopVersion": "0.2.10",
    "protocolVersion": 1,
    "systemId": "n64",
    "coreId": "mupen64plus-next",
    "coreBuild": "core-build",
    "contentHash": "rom-hash",
    "settingsHash": "settings-hash",
    "cheatsHash": "cheats-hash",
    "systemDataHash": null,
    "saveDataMode": "netplay"
  }
}
```

Sessions must not start if connected players have different fingerprints.

When both players match, the room enters `syncingState`.

## Snapshot Relay

Only the host can relay snapshot payloads. Snapshot chunks are validated for
size and then relayed to the guest:

```json
{
  "type": "snapshotChunk",
  "chunk": {
    "index": 0,
    "bytes": [1, 2, 3]
  }
}
```

Snapshot completion sends a manifest. The server validates total byte limits and
checksum format, but does not persist or reconstruct snapshot bytes:

```json
{
  "type": "snapshotComplete",
  "manifest": {
    "totalBytes": 123456,
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  }
}
```

## Ready And Start

After compatibility and snapshot sync, each client sends:

```json
{
  "type": "ready"
}
```

When both connected players are ready, the server broadcasts:

```json
{
  "type": "startSession",
  "startFrame": 0,
  "room": {}
}
```

## Input Frame

Every gameplay input packet must include:

```json
{
  "type": "inputFrame",
  "input": {
    "playerIndex": 0,
    "frame": 12345,
    "payload": [0, 1, 2, 3]
  }
}
```

Validation rules:

- The connection may only send input for its server-assigned `playerIndex`.
- Frame numbers must increase per player.
- Future frames are bounded by `MAX_FUTURE_FRAME_DISTANCE`.
- Input is only accepted while the room is `playing`.

Accepted input frames are broadcast as authoritative `inputFrame` messages.

## Snapshot Validation

Save-state sync payloads are treated as untrusted data.

Chunk validation:

- each chunk must be under `MAX_SNAPSHOT_CHUNK_BYTES`

Completed snapshot validation:

- byte length must match the manifest
- total bytes must be under `MAX_SNAPSHOT_BYTES`
- SHA-256 must match the manifest checksum

The server relays snapshot data but must not persist it.
