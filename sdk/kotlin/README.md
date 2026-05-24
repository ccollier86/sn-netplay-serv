# ShadowBoy Netplay Kotlin SDK

Pure Kotlin/JVM relay-contract SDK for ShadowBoy netplay clients.

The SDK owns protocol shapes and client-side relay state. Apps provide
transport, auth, emulator integration, and UI.

## Runtime Helpers

- `RoomStateMachine` ignores stale room epochs, tracks assigned player state,
  exposes reconnect tickets, and returns a diagnostics snapshot for logs/UI.
- `ResyncCoordinator` turns `StateHashMismatch` and `RecoveryResyncRequired`
  messages into a typed lifecycle: pause, clear prediction buffers, send/load
  snapshots, resend compatibility, wait for ready, and complete on
  `StartSession`.
- `RuntimeTelemetryTracker` builds heartbeat network samples from client-side
  frame, latency, jitter, stall, catch-up, late-input, and audio-underrun data.
- `StateHashReporter` owns report cadence, SHA-256 normalization, and duplicate
  frame suppression.
- Voice helpers cover optional LiveKit grants. `RoomStateMachine` stores the
  local private grant at `state.voice.privateGrant`, request renewal uses
  `RefreshVoiceToken`, and diagnostics intentionally omit the token.

Apps still own emulator-specific work: pausing the core, serializing/loading
save states, computing state hashes, and displaying progress.

## Local Test Command

The local JDK used for this SDK is:

```bash
JAVA_HOME=/home/catalyst-2/.local/jdk-21
```

There is no `gradlew` wrapper checked into this SDK folder. Use the installed
Gradle distribution with that `JAVA_HOME`:

```bash
JAVA_HOME=/home/catalyst-2/.local/jdk-21 /home/catalyst-2/.gradle/wrapper/dists/gradle-8.14.3-bin/cv11ve7ro1n3o1j4so8xd9n66/gradle-8.14.3/bin/gradle test
```
