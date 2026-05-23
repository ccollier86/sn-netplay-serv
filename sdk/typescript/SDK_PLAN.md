# TypeScript Netplay SDK Plan

## Purpose

The TypeScript SDK mirrors the Kotlin SDK for Desktop/Electron. It owns the
relay contract, but not Electron IPC, runner control, ROM lookup, auth session
storage, or UI.

## Responsibilities

- Typed REST requests and responses for room creation and room status.
- Typed WebSocket paths and client/server messages for protocol v3.
- JSON encoding/decoding with server-compatible field names and message tags.
- Reconnect token state and reconnect WebSocket query construction.
- Heartbeat state, stale detection, and recovery timeout detection.
- Runtime telemetry helpers for heartbeat RTT, jitter, local frame, stalls,
  catch-up frames, late inputs, and audio underruns.
- State-hash report cadence, validation, and duplicate-frame suppression.
- State-resync lifecycle helpers for pause, snapshot, compatibility, ready, and
  diagnostics flow.
- Coordinated pause/resume request ids, holders, acknowledgements, and resume.
- Room/player state machine driven by relay messages.
- Compatibility fingerprints, session descriptors, and client-side validation.
- Stable close/error reasons suitable for app-level recovery flows.

## Non-Responsibilities

- No Electron windows, IPC, menus, or renderer state.
- No emulator runner commands or save-state byte translation.
- No state hashing implementation; the app supplies serialized-state hashes.
- No ROM scanning or library persistence.
- No premium/license UI.
- No bundled HTTP/WebSocket implementation; adapters provide transport.

## Build

```bash
bun test sdk/typescript/tests/**/*.test.ts
bunx tsc --noEmit -p sdk/typescript/tsconfig.json
```
