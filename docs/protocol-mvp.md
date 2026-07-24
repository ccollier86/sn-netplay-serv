# Netplay Protocol V4/V5

The relay coordinates invite-code sessions for ShadowBoy clients. It never
runs emulator cores, never receives ROM data, and never streams gameplay video.
Clients run the emulators locally and use the relay for room discovery,
compatibility checks, save-state snapshot relay, input relay, coordinated pause,
heartbeat, and reconnect recovery.

The relay accepts protocol versions `4` and `5`. Room creation negotiates one
exact version from the client range, and every room WebSocket must repeat that
room's selected version.

## HTTP

### `GET /health`

Returns process health.

The response also carries immutable deployment identity and the supported
protocol range:

```json
{
  "status": "ok",
  "buildSha": "0123456789abcdef0123456789abcdef01234567",
  "imageIdentity": "ghcr.io/ccollier86/sb-netplay-serv:0123456789abcdef0123456789abcdef01234567",
  "version": "0.1.0",
  "minSupportedProtocolVersion": 4,
  "maxSupportedProtocolVersion": 5
}
```

### `POST /v1/rooms`

Creates a room for the verified host.

Headers:

```text
Authorization: Bearer <client-token>
X-Client-Kind: desktop | android | ios
X-Install-Id: <installationId>
X-Req-Ts: <epoch milliseconds>
X-Req-Nonce: <unique nonce>
X-Req-Sig: <base64 signature; Desktop, Android, or explicit iOS development>
X-App-Attest-Key-Id: <key id; production iOS only>
X-App-Attest-Assertion: <base64 assertion; production iOS only>
```

Desktop signs with the protected-client signer used for metadata, cheat,
billing, and update requests. Android sends `X-Client-Kind: android` and signs
with its protected install key. Production iOS sends `X-Client-Kind: ios` and
an App Attest key/assertion pair instead of `X-Req-Sig`. The relay forwards the
provider proof without interpreting or logging it. `X-Client-Kind` defaults to
`desktop` for older Desktop request shapes. `X-Installation-Id` is accepted as
an install-id alias.

The relay asks the metadata service to authorize feature `netplay`. Desktop
requests use `requiredEntitlement: "premiumOrTrial"`; Android and iOS requests
use `requiredEntitlement: "eligibleClient"` and leave premium gating inside the
app. The metadata service selects and validates the proof family from the
authenticated installation provider and rejects mixed proof families.

Body:

```json
{
  "desktopProtocolVersion": 4,
  "session": {
    "hostAppVersion": "0.3.0",
    "mode": "controllerNetplay",
    "game": {
      "systemId": "snes",
      "title": "Super Mario Kart",
      "romSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "contentKey": "snes-super-mario-kart-usa",
      "region": "USA",
      "revision": "Rev 1"
    },
    "core": {
      "coreId": "snes9x",
      "coreName": "Snes9x",
      "coreVersion": "informational-build",
      "coreOptionsSha256": null,
      "stateFormat": "snes9x:snes:s9x-freeze-stream-v1"
    },
    "controller": {
      "inputDelayFrames": 3
    },
    "link": null
  }
}
```

Descriptor fields must not contain absolute paths or local filenames. The ROM
hash is used only to match a local copy on the invited client. `coreVersion` and
`coreBuild` are informational; `stateFormat` is the hard compatibility gate for
snapshot bytes.

Controller rooms require exact `romSha256` and compatible `stateFormat`. Host
input delay is part of the room descriptor under `controller.inputDelayFrames`
and must be used by guests instead of local defaults.

> **Disabled until qualification:** the completed link descriptor, private
> per-room data plane, and SBLK v1 codec are admitted only when the client explicitly sends
> `linkContractVersion: 1` and the server has
> `SB_NETPLAY_LINK_CABLE_ENABLED=true`. The switch defaults to false. Link
> packets use targeted bounded queues and never enter shared room events,
> public event sequencing, or debug history. Physical-device qualification
> remains a production-enable blocker. See
> `docs/mgba-link-provider-foundation.md`.

Link-cable room creation adds `"linkContractVersion": 1` beside
`desktopProtocolVersion`, sets `mode: "linkCable"`, and uses exactly one of
these platform-neutral descriptors:

```json
{
  "systemFamily": "gba",
  "linkProtocol": "gba-sio-multi-v2",
  "runtimeProfile": "mgba-link-runtime-v1",
  "maxPlayers": 2,
  "transport": "relay"
}
```

GB and GBC use `systemFamily: "gb"` with
`linkProtocol: "gb-serial-v1"`. GBA uses `systemFamily: "gba"` with
`linkProtocol: "gba-sio-multi-v2"`. Both require `coreId: "mgba"` and exactly
two players. Link peers require matching runtime profile, core build id, and
supported link mode. ROM, BIOS/system-data, save, RTC, and full-state hashes are
not peer-equality fields, so the guest ROM does not have to match the host ROM.
The relay continues to decode the frozen `gba-sio-multi-v1` namespace for
explicit legacy descriptors, but normal GBA lobby negotiation selects v2.

Room responses include `eventSeq`, `roomEpoch`, and `sessionEpoch`:

```json
{
  "room": {
    "roomId": "<uuid>",
    "eventSeq": 0,
    "roomEpoch": 1,
    "sessionEpoch": 1,
    "inviteCode": "AB23-CD",
    "protocol": {
      "protocolVersion": 4,
      "minSupportedProtocolVersion": 4,
      "maxSupportedProtocolVersion": 5,
      "roomProtocolVersion": 4
    },
    "session": {
      "hostClientKind": "desktop",
      "hostAppVersion": "0.3.0",
      "roomMode": "directInvite",
      "mode": "controllerNetplay",
      "game": {
        "systemId": "snes",
        "title": "Super Mario Kart",
        "romSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "contentKey": "snes-super-mario-kart-usa",
        "region": "USA",
        "revision": "Rev 1",
        "discId": null
      },
      "core": {
        "coreId": "snes9x",
        "coreName": "Snes9x",
        "coreVersion": "informational-build",
        "coreOptionsSha256": null,
        "stateFormat": "snes9x:snes:s9x-freeze-stream-v1"
      },
      "controller": {
        "inputDelayFrames": 3
      },
      "link": null,
      "voice": null,
      "romIdentity": null,
      "romRelayIntent": "exactMatchOnly",
      "romRelay": null
    },
    "voice": null,
    "romRelay": null,
    "maxPlayers": 2,
    "pause": null,
    "frameClock": {
      "canonicalFrame": 0,
      "releasedFrame": null,
      "nextReleaseFrame": 0,
      "acceptedInputs": [
        {
          "playerIndex": 0,
          "frame": null
        }
      ],
      "pendingInputDelayChange": null
    },
    "status": "waitingForGuest",
    "players": [
      {
        "playerIndex": 0,
        "displayNumber": 1,
        "role": "host",
        "status": "connected",
        "runtimeState": "connected",
        "occupied": true,
        "controlConnected": false,
        "inputConnected": false,
        "supportsStateFileRelay": false,
        "supportsRomFileRelay": false,
        "supportsScheduledStart": false,
        "supportsClockSync": false,
        "supportsFastInputRelay": false,
        "lastSeenAgeMs": 20,
        "reconnectGraceRemainingMs": null
      }
    ]
  }
}
```

`roomEpoch` changes when membership or recovery state changes. `sessionEpoch`
changes when active gameplay must resync. Clients must include both values on
all non-ping WebSocket messages.

Player `runtimeState` values are relay-facing room view state. The relay may
infer `stale` when a connected player has missed heartbeat acknowledgements but
has not yet crossed the recovery timeout.

For readability, later control-message examples use `"$roomView"` as a
documentation macro for the complete `RoomView` object above. The wire value is
always that object, never the literal string.

### `GET /v1/rooms/{invite_code}/status`

Returns the current room view for a user-entered invite code. Guests should call
this before opening a WebSocket so they can show the game/core preview and block
unsupported local runtime combinations before launch.

## WebSocket

### Join

Host:

```text
GET /v1/ws?inviteCode=AB23-CD&role=host&protocolVersion=4
```

Guest:

```text
GET /v1/ws?inviteCode=AB23-CD&role=guest&protocolVersion=4
```

Link-room initial joins append the completed private-route contract:

```text
GET /v1/ws?inviteCode=AB23-CD&role=guest&protocolVersion=4&linkContractVersion=1
```

For a desktop-to-runner transfer, the protected provisional join adds:

```text
GET /v1/ws?inviteCode=AB23-CD&role=host&protocolVersion=4&runnerHandoff=true
```

`role` defaults to `guest` when omitted. Every initial join, including a runner
handoff join, requires the protected auth headers used for room creation.
`runnerHandoff` is rejected when combined with reconnect fields.

First successful socket message:

```json
{
  "type": "roomJoined",
  "eventSeq": 1,
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "yourPlayerIndex": 1,
  "resumeToken": "<opaque-token>",
  "inputSocketToken": "<opaque-input-token>",
  "voice": null,
  "room": "$roomView"
}
```

Controller clients keep both tokens in memory for the current room. They are
opaque capabilities and must not be logged, included in diagnostics, persisted
beyond the session, or shown to users.

A link room omits `inputSocketToken` and adds an authenticated private grant:

```json
{
  "type": "roomJoined",
  "eventSeq": 1,
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "yourPlayerIndex": 1,
  "resumeToken": "<opaque-token>",
  "voice": null,
  "linkCableGrant": {
    "contractVersion": 1,
    "roomScope": "4815162342",
    "roomEpoch": 2,
    "sessionEpoch": 2,
    "cableEpoch": 1,
    "localSlot": 1,
    "linkProtocol": "gba-sio-multi-v2",
    "maximumEventBytes": 128,
    "queueCapacity": 64,
    "status": "ready"
  },
  "room": "$roomView"
}
```

`roomScope` is a positive decimal string so its full 63-bit value is exact in
JavaScript and native clients. It is private control-plane admission data and
is not copied into SBLK, `RoomView`, public lobby output, room events, or logs.
While only one endpoint is attached, `status` is `waitingForPeer` and
`cableEpoch` may be zero. The server sends `linkCableGrantUpdated` after
lifecycle changes; `aborted` and `closed` grants include a safe
`failureReason`.

When `runnerHandoff=true`, the relay arms the provisional slot only for the
configured handoff grace period (60 seconds by default). The runner may claim
the slot before or after the provisional socket's close is processed. A late
close from the provisional socket cannot disconnect the runner. If capability
delivery fails, the handoff is cancelled and ordinary disconnect cleanup runs.

### Reconnect

Reconnect uses the invite code, the player slot, the last accepted room epoch,
and the resume token from `roomJoined`:

```text
GET /v1/ws?inviteCode=AB23-CD&protocolVersion=4&playerIndex=1&roomEpoch=4&resumeToken=<opaque-token>
```

Link-room reconnects must append `&linkContractVersion=1`. Missing and unknown
link contract versions are rejected before either initial or reconnect
admission. Controller-room initial joins and reconnects do not send this
parameter.

All three reconnect fields are required together. Partial reconnect queries are
rejected. A complete reconnect is authorized by this room-scoped capability and
does not use protected installation headers. A successful reconnect atomically
rotates `resumeToken`, returns a fresh `roomJoined`, and invalidates replay of
the presented token. Ordinary gameplay recovery returns the room to
compatibility checking and state sync before gameplay continues; runner handoff
does not add another room/session epoch transition.

### Binary Input Socket

Controller rooms use the control connection's fresh
`roomJoined.inputSocketToken` to attach the dedicated binary input socket:

```text
GET /v1/ws/input?inviteCode=AB23-CD&protocolVersion=4&playerIndex=1&roomEpoch=4&sessionEpoch=4&inputSocketToken=<opaque-token>
```

The input capability is the sole authorization for this endpoint. It is bound
to the control-connection generation that issued it and to the supplied player,
room epoch, and session epoch. Successful attachment consumes it, so replay
cannot replace the active input socket. If the input socket is lost during
gameplay, the relay starts bounded control recovery; reconnecting control is the
only way to obtain another input capability.

Resume and input capabilities appear in URL query parameters for the WebSocket
handshake. The relay's HTTP tracing records only the route path and never the
query string. Production rate limiting keys these capability routes by trusted
proxy IP or actual peer IP, never by an unverified installation header.

### Server Messages

State-carrying server messages include `eventSeq`, `roomEpoch`, and
`sessionEpoch`:

```json
{
  "type": "compatibilityRequested",
  "eventSeq": 3,
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "room": "$roomView"
}
```

Important server messages:

- `roomJoined`
- `roomStateChanged`
- `compatibilityRequested`
- `recoveryStarted`
- `startSession`
- `snapshotChunk`
- `snapshotComplete`
- `inputFrame`
- `linkCablePacket`
- `linkCableGrantUpdated`
- `sessionPauseScheduled`
- `sessionPauseUpdated`
- `sessionResumeScheduled`
- `heartbeatAck`
- `error`

## Compatibility

Controller-netplay clients send compatibility when the relay requests it:

```json
{
  "type": "setCompatibilityFingerprint",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "fingerprint": {
    "desktopVersion": "0.3.0",
    "protocolVersion": 4,
    "systemId": "snes",
    "coreId": "snes9x",
    "coreBuild": "informational-build",
    "stateFormat": "snes9x:snes:s9x-freeze-stream-v1",
    "contentHash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "settingsHash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "cheatsHash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "systemDataHash": null,
    "saveDataMode": "netplay"
  }
}
```

The relay compares protocol version, system id, core id, state format, content
hash, settings hash, cheats hash, system-data hash, and save-data mode. It does
not block only because app version or core build string differs.

Use these empty hashes when no deterministic data is present:

```text
settingsHash = SHA256("")
cheatsHash = SHA256("")
systemDataHash = null
```

If cheats are enabled and both clients cannot apply the exact same cheat set,
the mismatch should block the room.

Link-cable clients send:

```json
{
  "type": "setLinkCableCompatibility",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "compatibility": {
    "protocolVersion": 4,
    "systemFamily": "gba",
    "linkProtocol": "gba-sio-multi-v2",
    "runtimeProfile": "mgba-link-runtime-v1",
    "coreBuildId": "android-mgba-0.10.5-sb1",
    "supportedModes": ["multi"]
  }
}
```

GB/GBC clients instead send `systemFamily: "gb"`,
`linkProtocol: "gb-serial-v1"`, and `supportedModes: ["serial"]`. Initial and
resume control WebSocket URLs for either family must also include
`linkContractVersion=1`. This capability field is neither required nor emitted
for controller rooms.

## Snapshot Sync And Ready

For controller netplay, only the host can relay snapshot payloads. Chunks are
size-limited and relayed without persistence:

```json
{
  "type": "snapshotChunk",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "chunk": {
    "index": 0,
    "bytes": [1, 2, 3]
  }
}
```

Completion includes a manifest:

```json
{
  "type": "snapshotComplete",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "manifest": {
    "totalBytes": 123456,
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  }
}
```

Controller rooms cannot start until the host snapshot is complete. Link-cable
rooms can skip snapshot transfer and send `ready` after compatibility.

```json
{
  "type": "ready",
  "roomEpoch": 2,
  "sessionEpoch": 2
}
```

When every connected player is ready, the relay broadcasts:

```json
{
  "type": "startSession",
  "eventSeq": 8,
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "startFrame": 0,
  "room": "$roomView"
}
```

## State Hash Drift Repair

During controller netplay, clients periodically send deterministic state hashes
for the frame they just reached:

```json
{
  "type": "stateHash",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "report": {
    "frame": 6000,
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  }
}
```

The relay keeps a bounded per-room hash buffer. It first compares exact frame
reports. If exact-frame hashes differ, it also searches nearby frames in the
hash buffer so a client at frame 6000 can still be considered aligned with a
peer at frame 6005 when the serialized state hash matches.

The nearby-frame window is dynamic. The relay sizes it from fresh heartbeat
`localFrame` spread, with accepted input-frame cursors as a fallback, then adds
a small slack margin. The window is bounded so normal frame skew is recognized
without letting very old hashes hide a real deterministic desync.

Nearby-frame matches are diagnostics only and do not suppress recovery. On the
first confirmed exact-frame mismatch, the relay bumps the session epoch, moves
the room back to compatibility checking, and broadcasts `stateHashMismatch` with
a `repairFrame`. Clients must pause their active netplay runtime, resend
compatibility for the new session epoch, then run host snapshot sync for that
repair frame. Snapshot chunks and manifest carry the same `snapshotId` and
`repairFrame`. The host also loads that same retained state locally before
`ready`. Guests load it, reset rollback cursors to `repairFrame`, and send
`ready`. When both players are ready, the relay emits `startSession` using the
same repair frame.

## Input And Link Packets

Controller input:

```json
{
  "type": "inputFrame",
  "roomEpoch": 2,
  "sessionEpoch": 2,
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
- Future frames are bounded.
- Input is only accepted while the room is `playing` or inside the coordinated
  pause input-delay window.

Link-cable SBLK packet:

```json
{
  "type": "linkCablePacket",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "packet": {
    "playerIndex": 1,
    "sequence": 0,
    "emulatedTime": 72623859790382856,
    "payload": [
      83, 66, 76, 75, 1, 1, 0, 0,
      2, 0, 0, 0, 0, 0, 0, 0,
      2, 0, 0, 0, 0, 0, 0, 0,
      4, 0, 0, 0, 0, 0, 0, 0,
      0, 0, 0, 0, 0, 0, 0, 0,
      1, 13, 0,
      2, 3, 32, 0, 0, 8, 7, 6, 5, 4, 3, 2, 1
    ]
  }
}
```

`packet.payload` is one complete 48–128 byte SBLK frame from the exact protocol
namespace in the private grant. GBA v2 retains SBLK wire version 1 and its
43-byte header. The JSON player, sequence, and emulated-time fields must agree
with the decoded frame. Link packets are accepted only after a link room is
`playing`; each sender sequence starts at zero and must be the exact next value
for the current cable epoch. Packets enter only the opposite endpoint's bounded
queue and are never echoed to the sender. Malformed, spoofed, out-of-order, or
overflow traffic aborts the current cable generation and clears both per-room
queues instead of dropping a required event.

In `gba-sio-multi-v2`, every `MODE_SET` records that sender's latest mode
sequence. Ordered newer mode snapshots supersede older ones. The opposite slot
sends `MODE_ACK` kind 6 after native application; an ACK for an older sequence
is a valid no-op, while only the exact latest ACK releases that mode barrier.
`TRANSFER_START` is admitted only when both current snapshots report MULTI and
both latest snapshots are acknowledged. The first player may therefore wait in
MULTI indefinitely for the second player while the provider remains live; the
server does not impose a rendezvous timeout.

A slot-1 non-MULTI snapshot crossed with `TRANSFER_START` before
`TRANSFER_REPLY` nonfatally cancels that proposal in either server arrival
order. The exact-next transfer id is still consumed and both frames are
forwarded. A mode exit after REPLY remains a protocol violation. A valid COMMIT
enters a finish barrier; slot 1 sends `FINISH_ACK` kind 7 only after applying
the commit and firing native multiplayer completion. The transaction becomes
idle only after that exact transfer id is acknowledged.

A valid GBA or GB/GBC `TRANSFER_ABORT` is terminal for its cable generation: no
later packet is admitted, the exact abort frame is written to the peer first,
and only then does the server clear all remaining packet/transaction state and
publish the private `aborted` grant to both attached endpoints. Continuing link
traffic requires reattachment and a strictly newer `cableEpoch`.

## Coordinated Pause

Clients use coordinated pause for in-game menus, backgrounding, system pauses,
and any user action that stops the emulation loop. The goal is for both clients
to stop on the same frame and resume together.

Request:

```json
{
  "type": "requestSessionPause",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "requestId": "uuid-or-client-action-id",
  "reason": "menu",
  "localFrame": 900
}
```

The relay schedules a future pause frame and broadcasts `sessionPauseScheduled`.
Each client continues running until `pause.pauseAtFrame`, stops there, then
acknowledges:

```json
{
  "type": "sessionPauseReached",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "sequence": 1,
  "pausedAtFrame": 908
}
```

To resume, clients release their own pause holder:

```json
{
  "type": "requestSessionResume",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "requestId": "uuid-or-client-action-id",
  "reason": "menu",
  "sequence": 1
}
```

If a resume request arrives before every client acknowledged the pause, the
relay keeps the pause lifecycle active until all acknowledgements arrive, then
broadcasts `sessionResumeScheduled`.

Pause requests and resume requests are idempotent per player/request id.

## Heartbeat And Recovery

Clients send an app-level heartbeat during connected room lifetime:

```json
{
  "type": "heartbeat",
  "roomEpoch": 2,
  "sessionEpoch": 2,
  "latestEventSeq": 8,
  "localFrame": 12345,
  "runtimeState": "playing"
}
```

The relay replies:

```json
{
  "type": "heartbeatAck",
  "eventSeq": 8,
  "roomEpoch": 2,
  "sessionEpoch": 2
}
```

Heartbeat runtime states:

- `connected`
- `checkingCompatibility`
- `syncing`
- `ready`
- `playing`
- `pausing`
- `paused`
- `reconnecting`
- `disconnected`

If a playing or paused socket exceeds `SB_NETPLAY_HEARTBEAT_DISCONNECT_SECONDS`,
the relay moves the room to `recovering`, marks the slot `reconnecting`, bumps
both epochs, and requires compatibility plus state sync again after reconnect.
If recovery exceeds `SB_NETPLAY_RECONNECT_GRACE_SECONDS`, the room is removed.

## Persistent Lobby Return Messages

Desktop persistent lobbies use a separate lobby WebSocket above direct gameplay
rooms. When a child gameplay room ends, clients send `returnToLobby` with the
lobby epoch they observed when the game was active:

```json
{
  "type": "returnToLobby",
  "lobbyEpoch": 12,
  "proposalId": "00000000-0000-0000-0000-000000000001",
  "returnRequestedByPlayerIndex": 1,
  "reason": "playerRequestedReturn"
}
```

`returnRequestedByPlayerIndex` and `reason` are optional for backward
compatibility. Supported reasons are `playerRequestedReturn`, `runnerClosed`,
`remoteDisconnected`, `roomClosed`, `launchFailed`, `netplayError`,
`emulatorError`, and `runnerCrashed`.

The relay accepts the first valid return for the active pending launch, clears
the pending launch and readiness, and broadcasts `lobbyReturned` before the
generic `lobbyStateChanged`:

```json
{
  "type": "lobbyReturned",
  "eventSeq": 19,
  "lobbyEpoch": 13,
  "returned": {
    "proposalId": "00000000-0000-0000-0000-000000000001",
    "returnRequestedByPlayerIndex": 1,
    "reason": "playerRequestedReturn",
    "returnedAtMs": 1770000000000
  },
  "lobby": {}
}
```

A second player may report the same completed return after the first report has
already bumped the lobby epoch. The reducer treats that duplicate as idempotent
only when it matches the stored last return. A stale report is still rejected if
a new launch is active or if the selected proposal no longer matches.

## Internal Operator Endpoints

All internal endpoints require `Authorization: Bearer <SB_NETPLAY_ADMIN_TOKEN>`.

```text
GET /internal/metrics
GET /internal/rooms
GET /internal/rooms/{invite_code}
GET /internal/rooms/{invite_code}/events?limit=100
GET /internal/recent-events?limit=100
```

Event logs are sanitized. They include room ids, invite codes, event sequence,
epochs, kind, and detail, but not access tokens, resume tokens, ROM data,
snapshot bytes, or input payloads.

## Durable Telemetry

The relay can copy sanitized room events into Postgres for long-term netplay
analysis. This is intentionally separate from live room state:

- rooms, frame release, input relay, pause, and sync remain in process
- event capture is a bounded nonblocking queue write
- queue overflow drops telemetry instead of growing memory and increments
  `/internal/metrics.telemetryDroppedTotal`
- Postgres writes run in a background task and failures do not affect rooms

The Postgres event table receives one row per sanitized room event:

```sql
CREATE TABLE IF NOT EXISTS netplay_room_events (
  timestamp_ms BIGINT NOT NULL,
  room_id UUID NOT NULL,
  invite_code TEXT NOT NULL,
  event_seq BIGINT NOT NULL,
  room_epoch BIGINT NOT NULL,
  session_epoch BIGINT NOT NULL,
  kind TEXT NOT NULL,
  detail TEXT NOT NULL
);
```

The performance sample table receives one row per heartbeat/runtime sample:

```sql
CREATE TABLE IF NOT EXISTS netplay_performance_samples (
  timestamp_ms BIGINT NOT NULL,
  room_id UUID NOT NULL,
  invite_code TEXT NOT NULL,
  event_seq BIGINT NOT NULL,
  room_epoch BIGINT NOT NULL,
  session_epoch BIGINT NOT NULL,
  player_index SMALLINT NOT NULL,
  runtime_state TEXT NOT NULL,
  local_frame BIGINT,
  canonical_frame BIGINT NOT NULL,
  released_frame BIGINT,
  next_release_frame BIGINT NOT NULL,
  accepted_input_frame BIGINT,
  frame_delta BIGINT,
  round_trip_ms INTEGER,
  jitter_ms INTEGER,
  prediction_frames INTEGER,
  stall_count INTEGER,
  catch_up_frames INTEGER,
  late_input_frames INTEGER,
  audio_underruns INTEGER
);
```

Use the Postgres DSN in `SB_NETPLAY_POSTGRES_URL`:

```text
SB_NETPLAY_POSTGRES_URL=postgres://user:password@host:5432/database?sslmode=require
```

The relay keeps the password out of debug output. `sslmode=require` encrypts
transport without certificate-chain validation. Use `sslmode=verify-full` when
the metrics endpoint has a publicly trusted certificate chain.

Use `scripts/netplay-analytics.sh probe` after schema or credential changes to
verify the same Postgres writer used by the relay can persist one event and one
runtime sample. The command removes probe rows after a successful write so
normal reports stay clean. Use `scripts/netplay-analytics.sh report --limit 25`
for the normal operator view. The report summarizes each session independently
before computing multi-session averages and totals, including state-hash
matches, state-hash mismatches, resyncs, frame deltas, stalls, catch-up frames,
late inputs, and audio underruns. Use `raw recent` or `raw session` only when
the summarized report points to a room that needs deeper inspection. `raw
session --room <room_uuid>` returns recent epochs for that room; add `--epoch`
only when you need one specific resync epoch.
