# ShadowBoy Netplay Protocol V5 Input Lane

This directory is the canonical cross-platform wire contract for protocol v5
controller netplay. Android, iOS, and Desktop v2 must vendor the fixture files
unchanged and verify the manifest before release.

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

## Failure Rules

- Decode and validate the complete message before mutating room state.
- Reject unknown magic/discriminators, invalid lengths, invalid player indices,
  unsorted release cursors, wrong epochs, and wrong ownership.
- Ignore old duplicate input or host-open records idempotently.
- NACK a future input cursor; never synthesize missing input.
- Treat a future host-open cursor as a lane-integrity failure and recover.
- Close and recover when a bounded input receiver lags instead of dropping.

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
