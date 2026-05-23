# Android State Hash Resync Handoff

## Relay Behavior

The relay now treats deterministic state-hash drift in two stages:

1. Exact-frame mismatch with a nearby-frame match is frame skew only.
2. Repeated true mismatches with no nearby-frame match trigger a real resync.

The nearby-frame search window is dynamic. The relay sizes it from fresh
heartbeat `localFrame` spread, falls back to accepted input cursors, adds slack,
and caps the window. This avoids false desync when one client is a few frames
ahead of the other.

## SDK-Owned Pieces

Use the Kotlin SDK helpers instead of hand-rolling relay state:

- `RoomStateMachine.apply(message)` ignores stale room epochs and updates room,
  stale same-epoch event sequences, pause, reconnect, frame-clock, and resync
  state.
- `RoomStateMachine.diagnostics(nowMs)` returns the compact debug snapshot for
  logcat or a hidden debug view.
- `RoomStateMachine.effectivePauseReason()` distinguishes user pause, peer
  pause, recovery pause, and state-resync pause.
- `ResyncCoordinator` exposes decisions:
  - `shouldPauseEmulation()`
  - `shouldClearPredictionBuffers()`
  - `shouldSendHostSnapshot()`
  - `shouldWaitForSnapshot()`
  - `shouldRequestCompatibility()`
  - `shouldSendReady()`
- `RuntimeTelemetryTracker` builds heartbeat network samples.
- `StateHashReporter` owns hash cadence, lowercase SHA-256 normalization, and
  duplicate frame suppression. The cadence is elapsed-frame based, not exact
  modulo based, so skipped render/catch-up ticks still produce reports.

## Heartbeats And Telemetry

Every heartbeat should include the deterministic emulation frame. Prefer the
runtime telemetry tracker so counters are reset after each heartbeat:

```kotlin
telemetry.markLocalFrame(currentEmulatedFrame)
telemetry.setPredictionFrames(predictionFrames)
telemetry.recordRoundTrip(roundTripMs)
telemetry.recordCatchUpFrames(catchUpFramesSinceLastHeartbeat)
telemetry.recordStall(stallsSinceLastHeartbeat)
telemetry.recordLateInputFrames(lateInputsSinceLastHeartbeat)
telemetry.recordAudioUnderruns(audioUnderrunsSinceLastHeartbeat)

val heartbeat = roomStateMachine.heartbeat.heartbeatMessage(
    roomEpoch = state.roomEpoch,
    sessionEpoch = state.sessionEpoch,
    latestEventSeq = state.latestEventSeq,
    localFrame = null,
    runtimeState = ClientRuntimeState.Playing,
    telemetry = telemetry,
)
```

The relay drains this into server telemetry asynchronously. Android does not
know the durable analytics backend exists; it only sends accurate heartbeat
values.

## State Hash Reporting

Use the deterministic core/emulation frame that drives input replay. Do not use
wall-clock frames, rendered frames, audio frames, or relay frames.

```kotlin
if (stateHashReporter.shouldReport(currentEmulatedFrame)) {
    val message = stateHashReporter.stateHashMessage(
        roomEpoch = state.roomEpoch,
        sessionEpoch = state.sessionEpoch,
        frame = currentEmulatedFrame,
        sha256 = serializedStateSha256,
    )
}
```

Do not gate this on exact multiples yourself. The SDK intentionally reports once
the configured frame interval has elapsed since the last submitted hash.

## Runtime Input Epochs

Every relayed controller input frame must carry the batch/session epoch that came
from the input channel. The server binary input batch already contains these
values; preserve them when turning binary batches into SDK `ServerMessage`
instances:

```kotlin
ServerMessage.InputFrameMessage(
    roomEpoch = batch.roomEpoch,
    sessionEpoch = batch.sessionEpoch,
    input = inputFrame,
)
```

`RoomStateMachine.apply(message)` ignores stale input frames by epoch. Snapshot
chunks and snapshot completion messages now also include `roomEpoch` and
`sessionEpoch`, and they must match the current runtime epoch exactly. Android
should still avoid submitting ignored runtime payloads to the emulator runner;
apply the message first, or call `roomStateMachine.isRuntimeMessageCurrent(message)`
before touching emulator state.

## Required Resync Flow

When `roomStateMachine.state.resync != null`:

1. Pause emulation immediately.
2. Stop applying old input/server-frame work for the previous session epoch.
3. Clear local prediction, rollback, and state-hash buffers.
4. Resend compatibility for the new room/session epoch.
5. Host serializes a fresh current state and sends snapshot chunks plus manifest.
6. Guest receives and loads the snapshot.
7. Send `ready` only after local runtime state is clean.
8. Resume only after `ServerMessage.StartSession`.

The host must not keep running while guests are resyncing. Both clients should
show resync progress using the existing snapshot transfer UI.

## SDK Phase Hooks

Call these as the Android runtime advances:

```kotlin
roomStateMachine.resync.markPausing()
roomStateMachine.resync.markSnapshotNeeded()
roomStateMachine.resync.markSnapshotSendStarted()
roomStateMachine.resync.markSnapshotSendComplete()
roomStateMachine.resync.markSnapshotReceiveStarted()
roomStateMachine.resync.markSnapshotLoadStarted()
roomStateMachine.resync.markSnapshotLoadComplete()
roomStateMachine.resync.markCompatibilitySent()
roomStateMachine.resync.markFailed("snapshot-load-failed")
```

`StartSession` clears resync automatically through `RoomStateMachine.apply`.
After Android clears its own prediction/rollback buffers, call
`roomStateMachine.acknowledgeRuntimeReset()`.

## Compatibility Notes

The SDK does not load or translate save states. Android must still guarantee the
snapshot byte format is compatible for the advertised `stateFormat`.

Desktop-to-Android controller netplay remains gated by the app-level supported
system/core matrix. The server relays descriptors and enforces fingerprints; it
does not know which UI/client build supports a given core.
