# ShadowBoy Netplay TypeScript SDK

Pure TypeScript relay-contract SDK for the ShadowBoy Electron client.

The SDK owns protocol shapes and client-side relay state. Electron supplies
transport adapters, protected request signing, emulator integration, and UI.

## Runtime Helpers

- `RoomStateMachine` ignores stale room epochs, tracks assigned player state,
  exposes reconnect tickets, and returns a diagnostics snapshot for logs/UI.
- `ResyncCoordinator` turns `stateHashMismatch` and `recoveryResyncRequired`
  messages into a typed lifecycle: pause, clear prediction buffers, send/load
  snapshots, resend compatibility, wait for ready, and complete on
  `startSession`.
- `RuntimeTelemetryTracker` builds heartbeat network samples from client-side
  frame, latency, jitter, stall, catch-up, late-input, and audio-underrun data.
- `StateHashReporter` owns report cadence, SHA-256 normalization, and duplicate
  frame suppression.
- Voice helpers cover optional LiveKit grants. `RoomStateMachine` stores the
  local private grant at `state.voice.privateGrant`, request renewal uses
  `refreshVoiceToken`, and diagnostics intentionally omit the token.

Apps still own emulator-specific work: pausing the core, serializing/loading
save states, computing state hashes, and displaying progress.

```bash
cd /home/catalyst-2/projects/sb-desktop/sb-netplay-serv
bun test sdk/typescript/tests/**/*.test.ts
bunx tsc --noEmit -p sdk/typescript/tsconfig.json
```
