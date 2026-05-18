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
Authorization: Bearer <client-token>
X-Client-Kind: desktop | android
X-Install-Id: <installationId>
X-Req-Ts: <epoch milliseconds>
X-Req-Nonce: <unique nonce>
X-Req-Sig: <base64 signature>
```

Desktop should create the request signature using the same protected-client
signer it already uses for `/v1/desktop/*` metadata, cheat, billing, and update
requests. Android should set `X-Client-Kind: android` and sign with the Android
protected-client install key. `X-Client-Kind` defaults to `desktop` for older
Desktop builds. `X-Installation-Id` is accepted as an install-id alias.

The relay asks the metadata service to authorize feature `netplay`. Desktop
requests use `requiredEntitlement: "premiumOrTrial"`; Android requests use
`requiredEntitlement: "eligibleClient"` and leave feature/premium gating inside
the app.

Body:

```json
{
  "desktopProtocolVersion": 2,
  "session": {
    "hostAppVersion": "0.3.0",
    "mode": "controllerNetplay",
    "game": {
      "systemId": "gamecube",
      "title": "Star Fox Adventures",
      "romSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "contentKey": "gamecube-star-fox-adventures-usa",
      "region": "USA",
      "revision": "Rev 1",
      "discId": "GFSE01"
    },
    "core": {
      "coreId": "dolphin",
      "coreName": "Dolphin",
      "coreVersion": "5.0-netplay",
      "coreOptionsSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    },
    "link": null
  }
}
```

Descriptor fields must not contain absolute paths or local filenames. The ROM
hash is used only to match a local copy on the invited Desktop client. The relay
does not transfer ROM files.

`session.mode` defaults to `controllerNetplay` for older Desktop clients. Link
cable rooms must send `mode: "linkCable"` and a `link` descriptor:

```json
{
  "systemFamily": "gba",
  "linkProtocol": "gba-link-cable-v1",
  "runtimeProfile": "mgba-link-runtime-v1",
  "maxPlayers": 2,
  "transport": "relay"
}
```

The link descriptor is intentionally platform-neutral. Desktop and Android can
join the same room only when they can send the exact same `runtimeProfile` and
link protocol. Link rooms do not require guest ROM hashes to match the host
ROM hash.

Response:

```json
{
  "room": {
    "roomId": "<uuid>",
    "inviteCode": "AB23-CD",
    "protocol": {
      "protocolVersion": 2,
      "minSupportedProtocolVersion": 1
    },
    "session": {
      "hostAppVersion": "0.3.0",
      "mode": "controllerNetplay",
      "game": {
        "systemId": "gamecube",
        "title": "Star Fox Adventures",
        "romSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "contentKey": "gamecube-star-fox-adventures-usa",
        "region": "USA",
        "revision": "Rev 1",
        "discId": "GFSE01"
      },
      "core": {
        "coreId": "dolphin",
        "coreName": "Dolphin",
        "coreVersion": "5.0-netplay",
        "coreOptionsSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
      },
      "link": null
    },
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

Clients should call this before opening a WebSocket so the guest can see the
game title/system/core and verify local compatibility. Controller-netplay rooms
require an exact `romSha256` match. Link-cable rooms require a compatible local
runtime profile and may use different ROM hashes.

## WebSocket

### `GET /v1/ws?inviteCode=AB23-CD&role=host&protocolVersion=2`

Attaches the room creator's Desktop client to Player 1.

Required headers match room creation:

```text
Authorization: Bearer <client-token>
X-Client-Kind: desktop | android
X-Install-Id: <installationId>
X-Req-Ts: <epoch milliseconds>
X-Req-Nonce: <unique nonce>
X-Req-Sig: <base64 signature>
```

### `GET /v1/ws?inviteCode=AB23-CD&role=guest&protocolVersion=2`

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
    "protocol": {
      "protocolVersion": 2,
      "minSupportedProtocolVersion": 1
    },
    "session": {
      "game": {
        "systemId": "gamecube",
        "title": "Star Fox Adventures",
        "romSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "contentKey": "gamecube-star-fox-adventures-usa"
      },
      "core": {
        "coreId": "dolphin"
      }
    },
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
    "protocol": {
      "protocolVersion": 2,
      "minSupportedProtocolVersion": 1
    },
    "session": {
      "game": {
        "systemId": "gamecube",
        "title": "Star Fox Adventures",
        "romSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "contentKey": "gamecube-star-fox-adventures-usa"
      },
      "core": {
        "coreId": "dolphin"
      }
    },
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

Controller-netplay clients send these fields before the session can start:

```json
{
  "type": "setCompatibilityFingerprint",
  "fingerprint": {
    "desktopVersion": "0.2.10",
    "protocolVersion": 2,
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

## Link Cable Compatibility

Link-cable clients send a separate compatibility payload:

```json
{
  "type": "setLinkCableCompatibility",
  "compatibility": {
    "protocolVersion": 2,
    "systemFamily": "gba",
    "linkProtocol": "gba-link-cable-v1",
    "runtimeProfile": "mgba-link-runtime-v1",
    "systemDataHash": null
  }
}
```

The server compares protocol version, system family, link protocol, runtime
profile, and system-data hash. It does not compare ROM hashes for link-cable
rooms because trades and battles often use different game versions. When both
players are compatible, the room enters `syncingState`; each client can then
send `ready`.

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

## Link Cable Packet

Link-cable packets are only accepted for `linkCable` rooms after both clients
have sent `ready` and the room is `playing`.

```json
{
  "type": "linkCablePacket",
  "packet": {
    "playerIndex": 0,
    "sequence": 123,
    "emulatedTime": 582991,
    "payload": [1, 2, 3]
  }
}
```

Validation rules:

- The connection may only send packets for its server-assigned `playerIndex`.
- `sequence` must increase per player.
- `payload` must be non-empty and under `MAX_LINK_CABLE_PACKET_BYTES`.
- Packet bytes are opaque to the relay and are not persisted.

Accepted packets are broadcast as `linkCablePacket` messages to the other
connected player, not echoed to the sender.

## Snapshot Validation

Save-state sync payloads are treated as untrusted data.

Chunk validation:

- each chunk must be under `MAX_SNAPSHOT_CHUNK_BYTES`

Completed snapshot validation:

- byte length must match the manifest
- total bytes must be under `MAX_SNAPSHOT_BYTES`
- SHA-256 must match the manifest checksum

The server relays snapshot data but must not persist it.
