# ShadowBoy GBA link-cable v2 fixtures

This directory is the language-neutral byte contract for
`gba-sio-multi-v2`. The selected protocol remains out of band, so SBLK keeps
wire version `1` and the 43-byte header. Event bodies are fixed-width,
little-endian, and capped by the existing 128-byte frame limit.

V2 preserves the five v1 GBA event bodies byte-for-byte and adds:

- `MODE_ACK` (kind `6`, 16 bytes): the opposite slot's exact acknowledged
  `MODE_SET.senderSequence` as `u64`, followed by `emulatedTime` as `u64`.
- `FINISH_ACK` (kind `7`, 12 bytes): `transferId` as `u32`, followed by
  `emulatedTime` as `u64`; only slot 1 may send it.

All `u64` fields are restricted to the signed-63-bit range. The JSON envelope
time must equal the body timestamp for every timestamped event.

Shipping C, Kotlin, and Rust codecs must decode and re-encode every file in
`manifest.json` exactly. Same-language round trips without these fixtures are
not a protocol gate.
