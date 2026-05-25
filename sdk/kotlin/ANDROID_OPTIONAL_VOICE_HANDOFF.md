# Android Optional Voice Handoff

## Goal

Android multiplayer must continue to work without implementing voice chat,
without adding LiveKit, and without depending on the separate ShadowBoy Voice
SDK.

Voice is optional in the relay protocol. Android can safely leave it off while
still using invite-code multiplayer, state sync, pause/resume, reconnect, frame
clock, telemetry, and state-hash repair.

## Current Finding

The Android app currently does not model or send voice setup:

- `app/src/main/kotlin/app/suitengu/shadowboy/netplay/model/NetplayModels.kt`
  has no voice field on `NetplaySessionDescriptor`.
- `app/src/main/kotlin/app/suitengu/shadowboy/netplay/protocol/NetplaySdkAdapters.kt`
  converts Android sessions to SDK sessions without `voice`.
- `app/src/main/kotlin/app/suitengu/shadowboy/netplay/network/NetplayRelayHttpClient.kt`
  creates rooms from that descriptor, so Android-hosted rooms do not request
  voice.
- The embedded Kotlin SDK JSON config uses `ignoreUnknownKeys = true`, so room
  fields like `room.voice` and `roomJoined.voice` can be ignored by older
  Android code.

That is the correct voice-off behavior.

## Server Contract

The netplay relay only attempts voice setup when the host session descriptor has
voice enabled:

```json
{
  "session": {
    "voice": {
      "enabled": true,
      "mode": "voiceActivation"
    }
  }
}
```

If `voice` is absent, `null`, or has `enabled: false`, the relay must treat the
room as normal multiplayer with no voice room.

Server behavior in `sb-netplay-serv`:

- `NetplaySessionDescriptor.voice` is optional.
- `NetplayRoom.voice_requested()` returns true only when
  `session.voice.enabled == true`.
- Voice broker failures are non-fatal even when voice is requested.
- Gameplay room creation, compatibility, state sync, frame release, and
  reconnect are not blocked by voice availability.

## Android Required Behavior

For now, Android should do all of the following:

1. Do not add the ShadowBoy Voice SDK dependency.
2. Do not add LiveKit dependencies.
3. Do not request microphone permissions for multiplayer.
4. Do not show voice controls in Android multiplayer UI.
5. Do not include `session.voice` when creating rooms.
6. If Android later adopts the newer netplay SDK models, either omit
   `voice` or explicitly send:

```kotlin
voice = null
```

or:

```kotlin
voice = NetplayVoiceDescriptor(enabled = false)
```

7. Do not send `ClientMessage.RefreshVoiceToken`.
8. Ignore `RoomView.voice`.
9. Ignore `ServerMessage.RoomJoined.voice`.
10. If using the newer SDK `RoomStateMachine`, allow it to track
    `state.voice`, but do not pass the grant to any voice/media layer.

## Important Edge Case

If a Desktop host creates a voice-enabled room and Android joins it, the relay
may include voice metadata/grants in room messages. Android can still play
multiplayer without voice.

Android should simply ignore:

- `room.voice`
- `roomJoined.voice`
- voice grant diagnostics

Android must not request token refresh. The relay sends
`voiceTokenRefreshed` only in response to `refreshVoiceToken`; a voice-free
Android client should never trigger that path.

## SDK Guidance

The Kotlin netplay SDK may include optional voice helper types:

- `NetplayVoiceDescriptor`
- `RoomVoiceView`
- `PlayerVoiceJoinGrant`
- `NetplayVoiceGrantTracker`
- `RoomStateMachine.state.voice`

These are protocol compatibility helpers only. They do not require the Voice
SDK and do not connect to LiveKit.

The separate Voice SDK is only needed when Android intentionally implements
voice media:

- validating private LiveKit grants
- normalizing voice settings
- computing token refresh deadlines
- producing platform-neutral LiveKit adapter intents

Until then, Android should not include the voice SDK at all.

## Recommended Android Update Path

If Android updates its embedded netplay SDK to the newer version:

1. Copy the latest Kotlin netplay SDK.
2. Keep Android's local app model voice-free for now.
3. In `toSdk()` conversion, leave `voice = null`.
4. In room/message handling, let `RoomStateMachine.apply(message)` process
   optional voice fields without acting on `state.voice`.
5. Add no-op handling for `ServerMessage.VoiceTokenRefreshed` if using the
   newer sealed message type:

```kotlin
is ServerMessage.VoiceTokenRefreshed -> {
    roomStateMachine.apply(message)
    // No Android voice media yet; do not connect LiveKit.
}
```

6. Keep manual socket parsers tolerant of unknown message types if any old
   parser remains in use.

## Verification Checklist

Run these before and after updating the netplay SDK:

- Android hosts a room with no `voice` field in the create-room body.
- Android joins an Android-hosted room.
- Android joins a Desktop-hosted room where Desktop voice is disabled.
- Android joins a Desktop-hosted room where Desktop voice is enabled; gameplay
  still proceeds and Android simply has no voice.
- Android never sends `refreshVoiceToken`.
- No LiveKit dependency is present in the Android app.
- No microphone permission is requested by multiplayer.
- Unknown or unused voice metadata does not crash message decoding.

## Bottom Line

Android can leave voice completely off. The only required SDK is the netplay
SDK. The voice SDK and LiveKit adapter are future work for Android voice chat,
not prerequisites for Android multiplayer.
