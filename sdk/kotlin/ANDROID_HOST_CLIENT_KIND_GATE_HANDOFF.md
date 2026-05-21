# Android Netplay Host-Client-Kind Gate Handoff

## Goal

Temporarily block cross-platform controller netplay while allowing same-app
netplay:

- Android-to-Android: allowed.
- Desktop-to-Desktop: allowed.
- Android joining Desktop-hosted rooms: blocked for now.
- Desktop joining Android-hosted rooms: blocked for now.

This avoids starting sessions that can connect but later drift because Desktop
and Android still need cross-platform save-state/runtime compatibility work.

## Relay Contract

`RoomView.session` now includes:

```kotlin
val hostClientKind: NetplayClientKind?
```

`NetplayClientKind` values:

```kotlin
NetplayClientKind.Desktop // JSON: "desktop"
NetplayClientKind.Android // JSON: "android"
```

The relay stamps `hostClientKind` from verified auth on `POST /v1/rooms`.
Clients should not trust their request body as the source of truth.

## Android SDK Changes

The Kotlin SDK already has:

```kotlin
@Serializable
enum class NetplayClientKind {
    @SerialName("desktop")
    Desktop,

    @SerialName("android")
    Android,
}

@Serializable
data class NetplaySessionDescriptor(
    val hostClientKind: NetplayClientKind? = null,
    val hostAppVersion: String? = null,
    ...
)
```

Use the updated SDK model for preview, create-room responses, room status, and
WebSocket room messages.

## Required Android Request Headers

Android must send its client kind on every authenticated relay request:

```text
X-Client-Kind: android
```

This matters most on:

```text
POST /v1/rooms
GET /v1/ws?...
GET /v1/ws/input?...
```

If Android omits the header, the relay keeps legacy Desktop compatibility and
defaults the authenticated client kind to Desktop.

## Android Create Room

When Android creates a room, include this in the local descriptor for local UI
and tests:

```kotlin
NetplaySessionDescriptor(
    hostClientKind = NetplayClientKind.Android,
    ...
)
```

The relay will still overwrite/stamp the stored room descriptor from verified
auth.

## Android Join Gate

When previewing or joining an invite code:

```kotlin
val hostKind = room.session.hostClientKind

if (hostKind == NetplayClientKind.Desktop) {
    // Block before launching the ROM.
}
```

Recommended user-facing copy:

```text
Android cannot join Desktop-hosted netplay yet. Use Android-to-Android for now.
```

For `hostClientKind == null`, block with an update-style message rather than
launching cross-platform:

```text
This invite was created by an older netplay relay. Create a new invite and try again.
```

For `hostClientKind == NetplayClientKind.Android`, Android may continue its
normal Android-to-Android compatibility checks.

## Desktop Side

Desktop has already been gated. It rejects Android-hosted rooms during preview
and join with:

```text
Desktop cannot join Android-hosted netplay yet. Use Desktop-to-Desktop for now.
```

Implementation points:

- `packages/netplay-sdk/src/protocol/descriptors.ts`
- `apps/desktop/src/main/netplay/DesktopNetplayModeGuard.ts`
- `apps/desktop/tests/desktopNetplayModeGuard.test.ts`

## Relay Image

The relay image containing `hostClientKind` is:

```text
ghcr.io/ccollier86/sb-netplay-serv:latest
ghcr.io/ccollier86/sb-netplay-serv:e354c40
```

Expected digest:

```text
sha256:572229a3b1d488a419214405873d2eebaf4b83c0f2e6e4d5494b32e370cc0f3f
```
