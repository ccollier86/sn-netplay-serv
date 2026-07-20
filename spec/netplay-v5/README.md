# ShadowBoy Netplay Protocol V5 Input Lane

This directory is the canonical cross-platform wire contract for protocol v5
controller netplay. Android, iOS, and Desktop v2 must vendor the fixture files
unchanged and verify the manifest before release.

Protocol v5 is an exact room contract, not an optional message extension. Room
creation negotiates version 5 from the client's minimum and maximum supported
versions. Every control and input WebSocket join must then repeat version 5;
v4 sockets cannot attach to a v5 room and v5 sockets cannot downgrade in place.

## Transport Ownership

- The control WebSocket carries JSON lifecycle, compatibility, clock, snapshot,
  pause, recovery, heartbeat, and exit messages.
- The input WebSocket carries only the binary messages in this document plus
  WebSocket ping, pong, and close frames.
- One ordered writer owns each input socket. Input, retransmit, and host-open
  ordering must never depend on racing coroutines.
- Each client retains the latest 128 real local input frames until cumulatively
  acknowledged. The server accepts at most four consecutive frames per `SBI3`
  batch and rejects input more than 96 frames beyond its exact cursor.
- The server input broadcast is bounded. Falling behind closes and reconnects
  the input lane; it never silently drops a frame.

## Byte Order

All message-envelope integers are unsigned big-endian values. Controller input
uses the separately named `shadowboy-retropad-v1-le` codec and is exactly ten
bytes:

| Offset | Type | Meaning |
| ---: | --- | --- |
| 0 | `u16le` | Libretro joypad bits B through R3 |
| 2 | `i16le` | Left stick X |
| 4 | `i16le` | Left stick Y |
| 6 | `i16le` | Right stick X |
| 8 | `i16le` | Right stick Y |

Every envelope begins with a four-byte ASCII magic and a one-byte discriminator.
The discriminator is part of the stable contract even though the magic is
already unique.

## `SBI3` Strict Input Batch

| Offset | Size | Value |
| ---: | ---: | --- |
| 0 | 4 | ASCII `SBI3` |
| 4 | 1 | `0x03` |
| 5 | 8 | Room epoch |
| 13 | 8 | Session epoch |
| 21 | 1 | Zero-based player index |
| 22 | 1 | Frame count, 1 through 4 |
| 23 | 2 | Payload size, exactly 10 |
| 25 | 8 | First frame |
| 33 | `count * 10` | Consecutive fixed-size input payloads |

Frame numbers after the first are implicit. A sender flushes without a batching
timer; multiple records are for prefill, cumulative resend, or inputs already
available in the same runtime transaction.

## `SBA1` Input Cursor ACK

| Offset | Size | Value |
| ---: | ---: | --- |
| 0 | 4 | ASCII `SBA1` |
| 4 | 1 | `0x04` |
| 5 | 8 | Room epoch |
| 13 | 8 | Session epoch |
| 21 | 1 | Zero-based player index |
| 22 | 8 | Next input frame the server expects |

The next-expected cursor cumulatively acknowledges every lower frame.

## `SBN1` Input Cursor NACK

| Offset | Size | Value |
| ---: | ---: | --- |
| 0 | 4 | ASCII `SBN1` |
| 4 | 1 | `0x05` |
| 5 | 8 | Room epoch |
| 13 | 8 | Session epoch |
| 21 | 1 | Zero-based player index |
| 22 | 8 | Exact expected input frame |
| 30 | 8 | First rejected received frame |
| 38 | 1 | Reason code |

Reason codes are `1=input_gap`, `2=future_frame_too_large`, and
`3=session_state`. The expected cursor still cumulatively acknowledges lower
frames. Unknown reasons are protocol errors.

## `SBO1` Host Frame Open

| Offset | Size | Value |
| ---: | ---: | --- |
| 0 | 4 | ASCII `SBO1` |
| 4 | 1 | `0x06` |
| 5 | 8 | Room epoch |
| 13 | 8 | Session epoch |
| 21 | 8 | Exact frame opened by the host |

The host sends this on the same ordered writer after its input and required
start-of-frame state capture have been accepted for queueing.

## `SBF2` Server Frame Release

| Offset | Size | Value |
| ---: | ---: | --- |
| 0 | 4 | ASCII `SBF2` |
| 4 | 1 | `0x07` |
| 5 | 8 | Room epoch |
| 13 | 8 | Session epoch |
| 21 | 8 | Latest inclusive released frame |
| 29 | 8 | Next host frame expected by the relay |
| 37 | 1 | Accepted-input cursor count |
| 38 | `count * 9` | Cursor records |

Each cursor record is a one-byte zero-based player index followed by an
eight-byte next-expected input frame. Records must be unique and strictly
sorted by player index. These cursors are acknowledgement and diagnostics;
they never substitute for an `SBI3` payload.

## Frame Transaction

For each canonical frame, a client performs one ordered transaction:

1. Capture normalized local input and retain it by frame number.
2. Send `SBI3` immediately. Do not wait on a batching timer.
3. The host captures any required start-of-frame rollback state, then queues
   `SBO1` on the same writer after its input.
4. Apply `SBA1`/`SBN1` cumulatively and resend from the exact requested cursor.
5. Apply `SBF2` to the released host cursor. The exact first `SBF2` after a
   scheduled transition is a one-frame execution barrier: no core frame may
   run until that release arrives. After that first frame, simulate at the
   negotiated local core cadence while release, prediction, and sender bounds
   permit; do not wait for a per-frame relay round trip. Use real input where
   present and the normative predictor where remote input has not arrived.
6. During rollback replay, suppress both video and audio callbacks. Publish
   video and enqueue audio only for the final committed simulation.
7. Advance the local frame only after the emulator transaction succeeds.

The host-open cursor is the authoritative frame clock. A periodic timer may
wake work or detect a stall, but it must never release gameplay frames.

## Scheduled Start And Resume

ROM relay, initial host-state transfer, and local state loading complete before
a client begins the clock-sync generation used for scheduled start. Samples
and pongs belong to one room, session, preparation, and clock generation;
responses from any older generation are ignored. The relay emits
`startSession` with a `scheduledStart` containing the exact room/session epoch,
start frame, and future server time.

Before scheduling, the relay derives the initial input delay from fresh reports
for both data paths: host RTT / 2 + guest RTT / 2 + host jitter + guest jitter.
It converts that duration with the negotiated frame rate, adds one safety
frame, and clamps the result to 2 through 8 frames. If both fresh reports are
not available, the configured default is retained.

At the converted local deadline each client captures and queues the start
frame's input exactly once. The host also queues that frame's `SBO1` exactly
once, after its input. Neither client executes the core or advances to capture
the following frame. Only the `SBF2` for that exact room epoch, session epoch,
and start frame releases core execution. This first-release rule applies to
initial launch, pause resume, and state-recovery restart. It is a transition
barrier, not a conversion of predictive NES, SNES, or Genesis play into
per-frame lockstep.

A v5 pause is frame exact. Both clients execute pause frame `P` exactly once,
freeze published output, and settle pending rollback through `P`. A client may
acknowledge only after relay release and accepted-input cursors prove every
player complete through `P`; it retries that acknowledgement idempotently until
its player is listed in the room view. Once every holder releases,
`sessionResumeScheduled.resumeAtFrame` is `P + 1`. The relay increments the
session epoch and supplies a scheduled transition for `P + 1`; clients discard
all old-epoch input, open, and replay work and wait for the exact first `SBF2`
before executing it.

## Two-Phase State Recovery

SNES is the only predictive profile with authoritative V5 state digests. It
serializes start-of-frame checkpoints only at exact frames 600, 1200, 1800, and
so on. Reports may arrive later, but their frame remains the canonical
checkpoint. Authoritative decisions never match nearby frames: a same-frame
match continues and the first same-frame mismatch starts exactly one recovery.
After repairing frame `P`, the next checkpoint is `P + 600`. NES and Genesis
remain predictive without authoritative digests. N64 remains strict lockstep,
with digests disabled and no live state serialization. GameCube is outside the
V5 release.

Authoritative state digest mismatch uses a two-phase transaction:

1. `stateRecoveryPrepare` freezes the old session epoch at `repairFrame`.
2. The host serializes that exact start-of-frame state to durable local bytes.
3. The host sends `stateRecoveryPinned` in the old epoch with the exact
   `SnapshotManifest` from `fixtures/state-recovery-pinned.json`.
4. The relay validates the host, transaction id, frame, size, and digest. It
   then atomically emits `stateRecoveryCommitted` with a fresh session epoch.
5. Compatibility, exact pinned snapshot transfer, deterministic readiness, and
   scheduled release run in the fresh epoch. No substitute snapshot id, frame,
   byte count, or checksum is accepted.

The transaction is keyed by `recoveryId`. Duplicate prepare, pin, commit,
transfer, and start messages are idempotent. Prepare freezes and resets a
client runtime once; the host persists one checkpoint and resends the same pin.
Commit validates the ID, frame, and manifest without resetting again. A
reconnecting client hydrates the active transaction from
`RoomView.stateRecovery` before processing later events.

Initial launch transfer remains modal. During live recovery neither snapshot
upload, download, nor apply may publish blocking progress; clients show only
small repairing/restored notifications. Recovery is complete only after the
fresh epoch's first released frame has actually executed, not when
`startSession` is received.

The host has 10 seconds to pin state. A room permits two repair attempts in a
rolling 60-second window. Timeout emits reason `snapshotPinTimedOut`; exceeding
the attempt budget emits `recoveryAttemptLimitExceeded`; either failure closes
the gameplay room instead of leaving clients wedged.

`fixtures/state-recovery-prepare.json`,
`fixtures/state-recovery-pinned.json`, and
`fixtures/state-recovery-committed.json` are normative JSON payloads.

## Health Reports

V5 heartbeats retain the existing RTT, jitter, prediction, stall, catch-up,
late-input, and raw audio-underrun fields. Clients also report interval counters
for input resend frames, NACKs, replayed frames, suppressed audio/video frames,
audio catch-up operations, and trimmed audio frames, plus the current audio
queue depth. Optional audio diagnostics additionally report sustained rebuffer
events, maximum consecutive missing frames, and minimum/maximum queue depth.
All fields are optional for wire compatibility; production v5 clients populate
every counter they support.

N64 playback keeps a 6 ms fade for an isolated missing-audio gap without
de-priming the sink. Only 24 ms or more of consecutive missing audio enters the
existing 48 ms recovery prefill; a complete callback resets the gap accumulator
and recovery fades back in. Native Oboe and AudioTrack fallback use equivalent
policy. Solo playback is unchanged.

## Failure Rules

- Decode and validate the complete message before mutating room state.
- Reject unknown magic/discriminators, invalid lengths, invalid player indices,
  unsorted release cursors, wrong epochs, and wrong ownership, except for the
  bounded scheduled-transition drop rule below.
- Ignore old duplicate input or host-open records idempotently.
- NACK a future input cursor; never synthesize missing input.
- During a scheduled transition, including old-epoch work queued before the
  control transition became visible to the input lane, drop obsolete or
  otherwise inadmissible input/open work without ACK, NACK, cursor advancement,
  release advancement, or socket closure. Record the expected and received
  epochs, room status, frames, release cursor, and accepted-input cursors for
  diagnosis. Outside that bounded transition, enforce normal epoch and
  lane-integrity limits.
- Close and recover when a bounded input receiver lags instead of dropping.
- Close a v5 input socket that exceeds the transport token bucket rather than
  allowing duplicate-message abuse to monopolize room processing.

## Predictor Contract

The initial predictor ID is `shadowboy-retropad-predictor-v1`.

For a frame with no previous simulated value, copy the newest real input before
that frame or use ten neutral zero bytes when none exists.

For replay of a frame that already has a simulated value:

- preserve d-pad bits 4 through 7 from the previous simulated payload
- preserve all four signed analog axes from the previous simulated payload
- refresh every other joypad bit from the newest real input before the frame

Predicted bytes remain separate from real input and never advance an accepted
or confirmed cursor. `fixtures/predictor-vectors.json` is normative for this
behavior.
