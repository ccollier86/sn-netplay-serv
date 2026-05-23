# Netplay RetroArch-Parity Implementation Plan

This plan covers only differences that affect ShadowBoy multiplayer behavior:
frame sync, input exchange, prediction/rollback, state repair, pause/resume,
reconnect, and client runtime integration.

## Summary

Preserve ShadowBoy-specific product architecture: invite codes, auth, resume
tokens, telemetry, fixed player slots, and dual WebSockets. Change only the
pieces that can affect multiplayer correctness:

- one canonical frame stream
- contiguous input per player
- exact-frame state hash checks
- exact-frame snapshot repair
- matching TypeScript/Kotlin SDK protocol/state machines
- Desktop and Android runtime handoff guidance for matching rollback behavior

## Server And Protocol

- Add exact repair identity to snapshot transfers: `snapshotId`,
  `repairFrame`, `roomEpoch`, and `sessionEpoch`.
- Treat `serverFrame.frame` as the canonical released frame. `roomFrame` and
  `canonicalFrame` are diagnostics only.
- Keep accepted input contiguous per player. Old input is ignored; gaps are
  rejected.
- Compare deterministic state hashes only at exact matching frames.
- Nearby-frame hash matches are diagnostics only and must not reset mismatch
  state.
- Trigger repair on the first confirmed exact-frame mismatch.
- During active repair, send a host snapshot tied to one canonical repair frame.
- Emit session start with the repair frame, not `0`, when resuming from repair.
- Keep SHA-256 unless performance data proves it too expensive.
- Use 600-frame default state-hash cadence for normal builds; 60-frame cadence is
  diagnostics-only.
- Keep pause/reconnect recovery active while paused; do not let repair failure
  fall back into single-player.

## SDKs

- Update TypeScript and Kotlin types for exact-frame snapshot repair.
- Normalize TypeScript and Kotlin resync phases:
  `requested -> pausing -> snapshotNeeded -> snapshotSending/snapshotReceiving
  -> waitingForCompatibility -> waitingForReady -> complete`.
- Add helpers for canonical server-frame tracking and repair-frame validation.
- Keep SDK responsibility limited to protocol, heartbeat, reconnect, pause,
  room state, and diagnostics. Rollback/prediction stays in the emulator
  runtime.

## Desktop Runtime

- Start or restart netplay runtime from the server-supplied start frame.
- During repair, load the host snapshot and restart netplay mode from
  `repairFrame`; never use `startFrame = 0` for active repair.
- Keep current RetroArch-style prediction behavior:
  previous real input, direction duration preservation, non-direction refresh,
  replay when resolved input changes, 60-frame stall window, 2-frame resume
  margin, 3-frame catch-up threshold, 500ms catch-up probe.
- Add/keep tests proving repair does not leave stale room UI, stale invite data,
  or single-player fallback.

## Android Handoff

- Produce a handoff doc explaining the new exact-frame repair contract.
- Android must:
  - use `serverFrame.frame` as the canonical relay cursor
  - capture local input through `runFrame + inputDelayFrames`
  - report hashes only for synced exact frames
  - load repair snapshots at `repairFrame`
  - reset rollback cursors to `repairFrame` on repair
  - never continue as single-player after netplay repair or disconnect failure
  - gate unsupported core/state-format pairs before joining

## Test Plan

- Server room tests for exact hash mismatch, nearby diagnostics, repair frame
  selection, snapshot validation, and `startSession(repairFrame)`.
- TS/Kotlin SDK protocol codec tests for snapshot repair fields and matching
  resync phases.
- Desktop tests for state-hash repair flow, stale UI cleanup, and start frame
  propagation.
- Runner tests for prediction/replay constants and exact start-frame reset.
- Manual multiplayer checks: Desktop-to-Desktop NES/SNES/Genesis, Android-to-
  Android, and cross-platform only where state formats are confirmed compatible.

