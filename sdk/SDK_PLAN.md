# ShadowBoy Netplay SDK Plan

## Goal

Create small client SDKs that own the relay contract exactly once per language.
Desktop and Android should adapt emulator, UI, auth signing, ROM lookup, and
platform networking around the same typed protocol rules instead of each app
hand-writing relay calls.

## Shared Scope

- Typed room REST calls for create and status.
- Typed WebSocket paths, messages, and JSON codecs.
- Reconnect ticket handling with player slot, room epoch, and resume token.
- Heartbeat health and recovery timeout state.
- Runtime telemetry helpers for heartbeat RTT, jitter, local frame, stalls,
  catch-up frames, late inputs, and audio underruns.
- State-hash report cadence, validation, and duplicate-frame suppression.
- State-resync lifecycle helpers for pause, snapshot, compatibility, ready, and
  diagnostics flow.
- Coordinated pause/resume request and acknowledgement helpers.
- Room/player state machine driven only by relay messages.
- Controller and link-cable session descriptors.
- Compatibility fingerprints and link-cable compatibility values.
- Voice room descriptors, private voice grant tracking, token-safe diagnostics,
  and `refreshVoiceToken` message support. Media transport remains in the
  standalone voice SDK/LiveKit adapter.
- Stable app-level close/error reasons.

## Non-Scope

- No emulator core API calls.
- No save-state byte conversion.
- No state hashing implementation; apps supply serialized-state hashes.
- No Android, Desktop, or web UI.
- No ROM scanning, storage, or file permissions.
- No license purchase UI or entitlement decisions.
- No bundled network stack; apps provide transport adapters.

## Kotlin Phase

The Kotlin SDK comes first for Android integration. It is pure Kotlin/JVM,
targets Java 8 bytecode for Android compatibility, and uses caller-provided
HTTP/WebSocket/auth adapters.

## TypeScript Phase

The TypeScript SDK should mirror the Kotlin public shape for Desktop:
protocol DTOs, JSON codecs, REST/WebSocket request builders, reconnect state,
heartbeat state, pause helpers, and room state machine. Desktop-specific Electron
IPC and emulator runtime code stay outside the SDK.

## Verification

- Kotlin: `JAVA_HOME=/home/catalyst-2/.local/jdk-21 /home/catalyst-2/projects/gba-emulator/gradlew -p sdk/kotlin test`
- TypeScript: `bun test sdk/typescript/tests/**/*.test.ts`
- TypeScript typecheck: `bunx tsc --noEmit -p sdk/typescript/tsconfig.json`
