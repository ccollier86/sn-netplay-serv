# RetroArch Netplay Architecture Reference

This document records the RetroArch netplay model ShadowBoy is trying to match
where it applies to invite-code controller netplay. It is a technical parity
target, not a copy of RetroArch source code.

Reference source checkout used for this pass:

- `/tmp/RetroArch-netplay-review/network/netplay/README`
- `/tmp/RetroArch-netplay-review/network/netplay/netplay_private.h`
- `/tmp/RetroArch-netplay-review/network/netplay/netplay_frontend.c`
- `/tmp/RetroArch-netplay-review/config.def.h`
- `/tmp/RetroArch-netplay-review/configuration.c`

## Core Invariants

RetroArch netplay relies on a deterministic emulator core, identical content,
compatible serialized state bytes, and a controller/input surface that netplay
can model. If those conditions are true, the important invariant is:

Every input is applied to the same emulated frame on every client.

Network latency is handled by predicting missing remote input, then rewinding
and replaying when the real input arrives. The system is not normal lockstep.
The local emulator keeps running until it is too far ahead to repair within the
rollback window.

## State Buffer Model

RetroArch keeps a ring of frame records. The ring size is:

```text
NETPLAY_MAX_STALL_FRAMES + 2 = 62
```

Each ring entry is a `delta_frame` containing:

- resolved input that will be passed to the core
- real input received for each client
- simulated input generated for missing clients
- serialized core state before the frame runs
- frame number
- CRC-32 for that frame, when calculated
- `have_local`
- `have_real[client]`
- `used`

Important frame cursors:

- `self_frame_count`: the first local frame not yet fully actioned.
- `run_frame_count`: the frame actually being executed. This can lag behind
  `self_frame_count` when input latency is in use.
- `other_frame_count`: the first frame not proven synchronized. Frame
  `other_frame_count - 1` is the latest frame where all required input has been
  actioned consistently.
- `unread_frame_count`: the earliest frame where at least one connected player
  has not supplied input.
- `read_frame_count[client]`: next expected input frame for each client.
- `server_frame_count`: client-side view of the server sync clock.
- `replay_frame_count`: temporary replay cursor while rewinding forward.

The normal relationship is:

```text
other <= unread
other <= self
```

`unread` is usually behind `self`, but the model supports a peer/server getting
ahead.

## Server Frame Clock

RetroArch's server is the synchronization clock. It advances one frame at a
time and sends either its own input or an explicit `NOINPUT` for each frame. It
does not send frame `n + 1` while still synchronizing frame `n`.

Server behavior:

- Playing clients must send input for every frame.
- The server assigns the client number from the connection, not from the packet.
- Old or duplicate input is ignored.
- Future input with a gap is rejected as out of order.
- Input for a frame already reached by the server is forwarded immediately.
- Input for a future frame is stored and forwarded when the server reaches that
  frame.
- Spectators receive the same timing stream but do not send input.
- Player join/leave mode changes are scheduled against future frame numbers so
  all clients apply membership changes at the same emulated frame.

ShadowBoy cannot run the core on the relay, so our relay clock is only a
headless analog of this. It should still preserve the same properties: a single
canonical frame stream, contiguous accepted input per player, and no bulk
release of far-future frames.

## Input Packet Rules

RetroArch input sync expects one input packet per playing client per frame.

For each client:

```text
input.frame < read_frame_count[client]  => ignore duplicate/old packet
input.frame == read_frame_count[client] => accept and advance cursor
input.frame > read_frame_count[client]  => reject as out of order
```

This contiguous cursor rule matters. Allowing frame gaps makes later rollback
ambiguous because the clients no longer agree on what input is missing versus
what input has been skipped.

## Pre-Frame Flow

Before running a core frame, RetroArch:

1. Updates unread/read pointers.
2. Polls network input.
3. Polls/captures local input.
4. Serializes the current core state into the ring when needed.
5. Resolves input for the frame that will run.
6. Decides whether the emulator should stall instead of running.

If stalled or paused, netplay still performs its synchronization work, but the
core frame itself is not advanced.

## Local Input And Input Latency

RetroArch can read local input ahead of the frame being executed. This is how
it supports input latency frames:

```text
run_frame_count + input_latency_frames <= self_frame_count
```

If the client has not collected enough local input to satisfy the configured
latency, it stalls for `NETPLAY_STALL_INPUT_LATENCY`.

The first input frame is effectively neutral/zero because no previous input
exists yet.

## Prediction

When a remote player's real input for a frame is missing, RetroArch simulates it
from that player's previous real input.

On first simulation:

- copy the previous real input for that client/device

On resimulation after later real input arrives:

- joypad and analog d-pad directions keep the simulated duration
- non-direction buttons are refreshed from the newest real input

That distinction prevents button presses from creating repeated "wavefront"
presses during replay, while keeping directional movement duration close to
what happened before the real input arrived.

Resolved input is then built from the real or simulated per-client input for
each device.

## Post-Frame And Replay Flow

After the core frame runs, RetroArch checks whether new real input has arrived
for frames that were previously simulated.

If simulated input matches the real input:

1. Advance the synchronized cursor.
2. Keep running normally.

If simulated input changed, or a forced synchronized event happened:

1. Load the serialized state at the last synchronized frame.
2. Replay frames from that point to the current run frame using the corrected
   real input.
3. During replay, collect frame runtime samples.
4. Move `other_frame_count` to the latest frame that is now safe.

This is the heart of the RetroArch model: prediction is allowed to be wrong,
but wrong prediction must be corrected by state rewind and deterministic replay.

## Stall And Catch-Up Policy

RetroArch uses both stalling and catch-up. Stalling prevents the local emulator
from running so far ahead that rollback cannot repair it. Catch-up temporarily
lets a behind client run faster than normal.

Key behavior:

- If local `self_frame_count + 3 < lo_frame_count`, the client is behind.
- Before enabling catch-up, RetroArch waits for a 500ms probe to confirm the
  client is actually falling further behind.
- Catch-up exits when `self_frame_count + 1 >= lo_frame_count`.
- If local is too far ahead of unread input, it stalls.
- If the server sees one client ahead of others, it can request that client to
  stall for a bounded number of frames.
- Server-requested stalls are rate limited.
- Stalling does not time out while the peer is paused.

Constants:

| Name | Value | Purpose |
| --- | ---: | --- |
| `NETPLAY_MAX_STALL_FRAMES` | `60` | Maximum rollback/stall window. |
| ring buffer size | `62` | `MAX_STALL_FRAMES + 2`. |
| `NETPLAY_FRAME_RUN_TIME_WINDOW` | `120` | Moving average sample count for frame runtime. |
| `CATCH_UP_CHECK_TIME_USEC` | `500000` | Probe window before enabling catch-up. |
| `NETPLAY_MAX_REQ_STALL_TIME` | `60` | Max frames server can request a client to stall. |
| `NETPLAY_MAX_REQ_STALL_FREQUENCY` | `120` | Minimum frame spacing between requested stalls. |
| `MAX_SERVER_STALL_TIME_USEC` | `5000000` | Server-side stall timeout before hanging a client. |
| `MAX_CLIENT_STALL_TIME_USEC` | `10000000` | Client-side stall timeout before disconnecting. |
| `NETPLAY_PING_TIME` | `3000000` | Ping cadence once active. |
| `NETPLAY_ANNOUNCE_TIME` | `20000000` | LAN announce cadence. |
| `MAX_RETRIES` | `16` | Connection retry count. |
| `RETRY_MS` | `500` | Delay between connection retries. |

## Adaptive Input Latency

RetroArch has configurable input latency bounds:

- `netplay_input_latency_frames_min`
- `netplay_input_latency_frames_range`
- effective max = min + range

The default config values are both `0`, unless the user or command line sets
them. RetroArch then adjusts `input_latency_frames` within those bounds using a
moving average of frame runtime over 120 samples.

The built-in calculation assumes a `16666us` frame target. If replay capacity
cannot hide the network lead, it increases input latency up to the max. If
extra latency is no longer needed, it lowers it back toward the min.

This is not the same as letting the relay continuously retune a session delay.
RetroArch's adjustment is local and bounded by the host-provided latency range.

## CRC And Savestate Repair

RetroArch can periodically compare deterministic state hashes.

Default CRC cadence:

```text
DEFAULT_NETPLAY_CHECK_FRAMES = 600
```

Server side:

1. On frames divisible by `check_frames`, calculate CRC-32 of that exact frame's
   serialized state.
2. Send `CRC(frame, hash)`.

Client side:

1. If the client still has that exact frame in the ring, compare immediately.
2. If the client has not replayed that frame yet, store the received CRC in the
   frame record and check when it catches up.
3. If the first CRC check fails, mark CRCs as invalid for that core/session.
4. If a later valid CRC mismatches, request a savestate.

Repair flow:

1. Client sends `REQUEST_SAVESTATE`.
2. Server delays sending until the next pre-frame so the state is not sent after
   input for the same frame.
3. Server sends `LOAD_SAVESTATE(frame, uncompressed_size, bytes)`.
4. Client requires the load frame to match its server-frame cursor.
5. Client writes the received state into the ring at that frame.
6. Client marks `force_rewind`.
7. Client advances stale read cursors to the load frame.
8. Post-frame replay loads that state and replays forward.

The load frame is part of the protocol. A state payload without the frame it
belongs to is not equivalent.

## Pause And Resume

RetroArch has explicit pause and resume commands.

- A local pause sends `PAUSE`.
- A remote pause marks the peer as paused and pauses locally.
- `RESUME` clears the remote pause.
- Stall timeouts are suppressed while the remote side is paused.

Pause is a synchronization state, not just UI state.

## Handshake And Compatibility

Handshake outline:

1. Both sides exchange and validate the connection header.
2. Both sides exchange nicknames.
3. Client sends password if required.
4. Server sends content/core `INFO`.
5. Client sends content/core `INFO`.
6. Server sends `SYNC`.

`INFO` includes content CRC, core name, and core version. If server/client
cannot agree, the right outcome is disconnect/refuse.

`SYNC` gives the joining client:

- frame number
- paused bit
- assigned client number
- controller devices
- share modes
- controller/client mapping
- nick
- SRAM data

## Command Surface

Important command groups:

- Connection: `ACK`, `NAK`, `DISCONNECT`
- Input clock: `INPUT`, `NOINPUT`
- Handshake: `NICK`, `PASSWORD`, `INFO`, `SYNC`
- Participation: `SPECTATE`, `PLAY`, `MODE`, `MODE_REFUSED`
- State integrity: `CRC`, `REQUEST_SAVESTATE`, `LOAD_SAVESTATE`
- Runtime coordination: `PAUSE`, `RESUME`, `STALL`, `RESET`
- Optional surfaces: `PLAYER_CHAT`, `PING_REQUEST`, `PING_RESPONSE`,
  `NETPACKET`
- Shared settings: `SETTING_ALLOW_PAUSING`,
  `SETTING_INPUT_LATENCY_FRAMES`

## ShadowBoy Parity Map

Already aligned or partially aligned:

- Dedicated input WebSocket resembles RetroArch's input-clock channel.
- Relay emits `serverFrame`, which is ShadowBoy's `NOINPUT`-style clock tick.
- Desktop runner uses rollback/prediction instead of pure lockstep.
- Max stall window is `60` frames.
- Catch-up probe uses the same `3` frame threshold and `500ms` probe shape.
- The relay now enforces contiguous input cursors per player.
- The relay now releases frames from the host input cursor instead of waiting
  for every player, so peer input can arrive late and be corrected by rollback.
- State drift checks are frame-specific and can request a resync.

Known gaps to close:

- ShadowBoy repair currently uses room-level snapshot/resync messages, not a
  strict `REQUEST_SAVESTATE` -> `LOAD_SAVESTATE(frame, bytes)` equivalent.
- Snapshot messages need explicit frame semantics everywhere. Epochs help with
  stale sessions, but they are not a substitute for the load frame.
- The server is a headless relay, so the host input cursor is our practical
  clock source. It still must never bulk-release future frames.
- Hash comparison must remain tied to exact frames. Nearby-frame matching can
  help diagnose drift, but repair must load a single canonical frame.
- ShadowBoy uses SHA-256 for state checks instead of RetroArch CRC-32. That is
  fine for correctness but heavier and should be sampled carefully.
- Device sharing is intentionally simpler: fixed ShadowBoy player slots instead
  of RetroArch's full shared-device modes.
- Adaptive input delay is not currently equivalent to RetroArch. RetroArch uses
  local bounded latency adjustment, not relay-wide continuous delay changes.
- Android and Desktop must have equivalent rollback/replay semantics. Matching
  protocol messages is not enough if the runner loop differs.

## ShadowBoy Implementation Checklist

1. Keep relay input acceptance contiguous per player.
2. Keep relay frame release host-clocked and one frame at a time.
3. Do not require guest input before releasing a host frame.
4. Predict missing remote input client-side from the previous real input.
5. Rewind and replay when later real input changes a predicted frame.
6. Track state hashes by exact frame.
7. Add repair messages with explicit load-frame semantics.
8. During repair, load the host state into the exact frame buffer and replay
   forward rather than restarting local single-player flow.
9. Treat pause/resume as synchronized runtime state and suppress connection
   timeouts while peer-paused.
10. Keep Android and Desktop runners using the same counters and state
    transition meanings:
    - local/self frame
    - canonical/server frame
    - last synchronized frame
    - peer unread/read cursor
    - replay cursor

## Practical Defaults For ShadowBoy

Until we intentionally diverge, these are the safest defaults:

- rollback/stall window: `60` frames
- frame buffer size: `62`
- catch-up enter threshold: more than `3` frames behind
- catch-up probe: `500ms`
- catch-up exit threshold: within `1` frame of canonical/unread cursor
- server-requested stall max: `60` frames
- server-requested stall frequency: no more than once per `120` frames
- state hash cadence: start at `600` frames, then tune from telemetry
- connection ping cadence: about `3s`
- disconnect while stalled: server `5s`, client `10s`, suppressed while paused

Any future adaptive behavior should be introduced behind diagnostics and must
not violate the one-frame-at-a-time clock and exact-frame repair rules.
