# Kotlin Netplay SDK Plan

## Purpose

The Kotlin SDK owns the ShadowBoy relay contract without owning Android UI,
emulator runtime, ROM storage, signing implementation, or platform networking.
Android supplies adapters for auth, HTTP, WebSocket IO, emulator snapshots,
input sampling, and UI.

## Responsibilities

- Typed REST requests and responses for room creation and room status.
- Typed WebSocket client/server messages for protocol v3.
- JSON encoding/decoding with server-compatible field names and message tags.
- Reconnect token state and reconnect WebSocket query construction.
- Heartbeat state, stale detection, and recovery timeout detection.
- Coordinated pause/resume request ids, holders, acknowledgements, and resume.
- Room/player state machine driven by relay messages.
- Compatibility fingerprints, session descriptors, and client-side validation.
- Stable close/error reasons suitable for app-level recovery flows.

## Non-Responsibilities

- No Android views, lifecycle classes, services, or permissions.
- No emulator core APIs or save-state byte translation.
- No ROM scanning or file access.
- No premium/license UI.
- No bundled HTTP/WebSocket implementation; adapters provide transport.

## Build

Until this repo has its own Gradle wrapper, verify with:

```bash
JAVA_HOME=/home/catalyst-2/.local/jdk-21 /home/catalyst-2/projects/gba-emulator/gradlew -p sdk/kotlin test
```
