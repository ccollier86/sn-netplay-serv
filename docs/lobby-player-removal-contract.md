# Lobby Player Removal Contract

Status: implemented server contract; desktop adoption pending

Owner: `sn-netplay-serv`

Consumers: ShadowBoy Android, ShadowBoy Desktop v1, ShadowBoy Desktop v2,
ShadowBoy iPhone, and `sb-webrtc`

## Purpose

A lobby host must be able to remove an occupied guest slot while the lobby is
in setup. Removal is authoritative on the netplay relay. A client must never
remove a player only from its local roster.

This contract deliberately calls the operation `removePlayer`. UI may use the
friendlier label `Remove from lobby`. `Kick` may remain an internal UI action
name, but it is not the wire name.

## Shipped Implementation

The server and Android portions of this contract are implemented at these
immutable revisions:

| Component | Revision | Deployment artifact | Published index digest |
| --- | --- | --- | --- |
| `sn-netplay-serv` | `b085007` | `ghcr.io/ccollier86/sb-netplay-serv:4db189f` | `sha256:487a87959fddd9f12e50df83a0a349ec7bb1645753a735df023f9dba1a41b602` |
| `sb-webrtc` | `39e5320` | `ghcr.io/ccollier86/sb-webrtc:dfc3767` | `sha256:f539ae00bdac678079a182f346ea49eb41d392821d804bd71ab5d90be069d157` |
| ShadowBoy Android | `9f3f476a` | version code 232 / `1.11.23-dev` | n/a |

The later server image revisions only pin and configure reproducible AMD64
container builds; the feature revisions above contain the protocol behavior.
Each immutable tag and its matching `latest` tag resolve to the digest shown
above. Each index has one runnable `linux/amd64` manifest. Its additional
`unknown/unknown` manifest is OCI attestation metadata, not an ARM image.

Desktop v1 and v2 must adopt the wire contract and runtime behavior below.
Desktop v2 already has a `Remove from lobby` presentation item and computes a
`can_kick` value, but it must remain capability-gated until its action reaches
the relay. A visible action that cannot reach the relay is a bug.

## User Contract

- Only the current lobby host can remove another player.
- The host cannot remove itself or the host slot.
- Only occupied guest slots are valid targets.
- Connected and reconnecting guests can both be removed.
- The action is available while the lobby is in setup and no launch is in
  progress.
- The action is unavailable after a child gameplay launch begins. In-game
  player termination is a separate synchronized gameplay operation.
- UI must show a destructive confirmation naming the guest before sending the
  command.
- The removed guest returns to multiplayer home and sees `The host removed you
  from the lobby.`
- Removal invalidates the old lobby resume token immediately.
- Removal is not a permanent account or lobby ban. A user who still has a
  valid invite code may join again as a new membership. Ban support, if ever
  required, needs a separate subject-based policy and expiry contract.

## Capability Negotiation

Add these backward-compatible capability fields:

```json
{
  "supportsLobbyPlayerRemoval": true,
  "supportsLobbyPlayerRemovedEvent": true
}
```

`supportsLobbyPlayerRemoval` is a server capability in
`LobbyServerCapabilities`. Host clients must not show the removal action when
it is absent or false.

`supportsLobbyPlayerRemovedEvent` is a client capability in
`LobbyClientCapabilities`. It tells the relay that the client understands the
targeted `playerRemoved` event. Missing fields decode as false.

The relay remains authoritative even when the target is an older client. For
an older target it sends the existing `error` envelope with code
`removedFromLobby`, then closes that socket. Its invalidated resume token
prevents membership recovery.

## Wire Contract

Lobby player indexes are zero-based on the wire, matching all existing lobby
messages and lobby views.

### Host Request

```json
{
  "type": "removePlayer",
  "lobbyEpoch": 7,
  "playerIndex": 1
}
```

Rust model:

```rust
LobbyClientMessage::RemovePlayer {
    lobby_epoch: u64,
    player_index: u8,
}
```

Kotlin model:

```kotlin
@SerialName("removePlayer")
data class RemovePlayer(
    val lobbyEpoch: Long,
    val playerIndex: Int,
) : LobbyClientMessage
```

TypeScript model:

```ts
type RemovePlayerMessage = {
  readonly type: "removePlayer";
  readonly lobbyEpoch: number;
  readonly playerIndex: number;
};
```

### Targeted Removal Event

Only the removed socket receives this terminal event:

```json
{
  "type": "playerRemoved",
  "eventSeq": 18,
  "lobbyEpoch": 8,
  "playerIndex": 1,
  "reason": "removedByHost",
  "lobby": {}
}
```

`lobby` is the authoritative post-removal view. The target slot is already
empty in this view.

Rust model:

```rust
LobbyServerMessage::PlayerRemoved {
    event_seq: u64,
    lobby_epoch: u64,
    player_index: u8,
    reason: LobbyPlayerRemovalReason,
    lobby: LobbyView,
}
```

The initial reason enum contains one value:

```text
removedByHost
```

The returned `lobbyEpoch` is the post-mutation epoch, so it is newer than the
epoch in the accepted request. The server sends the targeted terminal event
before the normal `lobbyStateChanged` broadcast for the same post-removal
state. The removed session sends the terminal event and exits its WebSocket
loop. Other sessions ignore the targeted domain event and then consume the
roster update.

No separate success response is required for the host. The authoritative
`lobbyStateChanged` message is the command acknowledgement.

## Stable Errors

The request uses existing `error` envelopes and these codes:

| Code | Meaning |
| --- | --- |
| `staleLobbyEpoch` | The host acted on an old lobby view. |
| `hostOnly` | The requesting connection is not the host. |
| `invalidLobbyPlayerIndex` | The target index is outside the lobby slot range. |
| `lobbyPlayerNotFound` | The target slot is empty. |
| `cannotRemoveLobbyHost` | The target is the host slot. |
| `lobbyPlayerRemovalUnavailable` | Launch preparation or gameplay makes removal unsafe. |
| `unknownLobbyConnection` | The requester no longer owns a lobby connection. |

An error changes no lobby state and leaves the menu usable for another
attempt.

## Relay State Transition

The registry performs removal under the existing lobby write lock:

1. Validate `lobbyEpoch`.
2. Resolve the requesting connection and require the host role.
3. Resolve the target and require an occupied guest slot.
4. Require setup state with no pending launch or active gameplay.
5. Capture the target connection id and voice participant identity, if present.
6. Replace the target with a new empty slot. This erases subject ownership,
   client capabilities, connection id, and resume-token hash.
7. Remove all readiness rows for the target.
8. Leave the selected game intact and restore status to `gameSelected`, or
   `open` when no game is selected.
9. Bump lobby event sequence and meaningful activity exactly once.
10. Emit a targeted `LobbyEvent::PlayerRemoved` when the guest has an active
    connection.
11. Emit the normal `LobbyStateChanged` event and public-directory update.
12. Record a sanitized `lobbyPlayerRemoved` diagnostic containing only the
    target player index and connection state.

The removed socket eventually runs its normal disconnect cleanup. Because the
slot is already empty, that cleanup is intentionally a no-op and must not bump
the lobby a second time.

## Associated Cleanup

### Client Runtime

On a local `playerRemoved` event, every client implementation must:

- classify the lobby close as terminal `removedByHost`;
- stop automatic lobby reconnect;
- erase the resume token and active lobby membership;
- cancel game selection, readiness, ROM/start-state transfer, and launch work;
- disconnect lobby voice;
- close any chat or lobby modal owned by that membership;
- navigate to multiplayer home and present the removal message.

### Voice

Client disconnect is the primary immediate cleanup. The relay also adds a
best-effort trusted broker operation:

```text
DELETE /v1/voice/rooms/{voiceRoomId}/participants/{participantIdentity}
```

`sb-webrtc` validates that the identity belongs to the room and calls LiveKit
RoomService `RemoveParticipant`. Voice cleanup failure never rolls back the
authoritative lobby removal; it is logged and metered for retry/diagnosis.

This is session removal, not an adversarial credential ban. Existing LiveKit
JWTs are short-lived and the supported clients stop reconnecting after the
terminal lobby event. Hard token revocation would require rotating the lobby
voice room and all remaining grants, which is outside this first contract.

### File Relay

The removed client cancels active upload/download work locally. The lobby relay
rejects all new grants because the old connection no longer owns a slot.
Already-issued temporary file grants retain their existing short expiry. If
immediate grant revocation becomes a security requirement, add a transfer
cancellation API to the file broker as a separate contract rather than hiding
that behavior inside player removal.

### Child Gameplay Rooms

Removal is rejected once lobby launch preparation is pending or gameplay is
active. This avoids orphaning a published child room and keeps synchronized
in-game termination under the existing return-to-lobby protocol.

## Repository Implementation Map

### `sn-netplay-serv`

- `src/protocol/lobby_messages.rs`: request and targeted event DTOs.
- `src/lobbies/lobby_capabilities.rs`: client/server capability fields.
- `src/lobbies/lobby.rs`: host-authorized domain transition.
- `src/lobbies/lobby_event.rs`: targeted removal domain event.
- `src/lobbies/lobby_registry_trait.rs`: registry operation.
- `src/lobbies/in_memory_lobby_registry.rs`: atomic mutation, broadcasts,
  diagnostics, public directory, and voice cleanup scheduling.
- `src/lobbies/errors.rs` and transport error mapping: stable errors.
- `src/transport/websocket_lobby_session.rs`: request dispatch, targeted event
  delivery, legacy fallback, and socket termination.
- `docs/lobby-player-removal-contract.md`: canonical contract.

The lobby contracts currently copied into downstream SDK repositories must be
kept byte-for-byte compatible with the server JSON even where the server's
older bundled SDK directory has not yet adopted persistent lobbies.

### `sb-webrtc`

- Trusted participant-removal request routing and API documentation.
- `VoiceProvider` and `LiveKitVoiceProvider` participant removal.
- Service validation, telemetry, metrics, and provider tests.

### Android / Kotlin SDK

- `sdk/kotlin/.../protocol/LobbyMessages.kt`: request and event.
- `sdk/kotlin/.../protocol/Lobbies.kt`: capability fields and reason enum.
- `sdk/kotlin/.../state/LobbyStateMachine.kt`: terminal removed state.
- Android lobby coordinator: `removePlayer(playerIndex)` command and terminal
  cleanup.
- v2 lobby mapper: expose removal only for host, guest target, server support,
  and idle launch state.
- player menu: destructive confirmation followed by the real command.

These items are implemented in Android revision `9f3f476a`. The installed dev
build advertises `supportsLobbyPlayerRemovedEvent`, only exposes the action to
the host for occupied guest slots, sends the current lobby epoch, and treats
local removal as terminal so the client cannot reconnect with stale state.

### Desktop v1 / TypeScript SDK

- `packages/netplay-sdk/src/protocol/lobbyMessages.ts`: matching wire types.
- `packages/netplay-sdk/src/protocol/lobbies.ts`: matching capabilities.
- `packages/netplay-sdk/src/state/lobbyStateMachine.ts`: terminal removal.
- Desktop lobby coordinator and renderer: send the request, clear all lobby
  services when removed, and expose the host action with confirmation.

### Desktop v2

- Keep `LobbyPlayerContextMenu.svelte` as presentation only.
- Add `MultiplayerCommand::RemoveLobbyPlayer` and
  `MultiplayerAction::RemoveLobbyPlayer` to the Rust runtime contract.
- Project the command through the app-owned lobby service to the relay.
- Gate `can_kick` on server capability, local host role, target guest role, and
  launch state.
- Wire the existing frontend `kick` action to the runtime command and add the
  confirmation interaction.
- Handle terminal local removal in the runtime event projection and frontend
  store.

## Desktop Adoption Checklist

Apply this checklist to both desktop SDK/runtime generations. The UI layer must
not implement roster mutation itself.

1. Add both capability fields with a decode default of false. Advertise
   `supportsLobbyPlayerRemovedEvent: true` only after terminal cleanup is
   implemented.
2. Encode `removePlayer` with the target's zero-based `playerIndex` and the
   `lobbyEpoch` from the currently rendered authoritative lobby view.
3. Decode `playerRemoved`, including its post-mutation lobby view and
   `removedByHost` reason. Preserve unknown future reasons without crashing the
   lobby event loop.
4. Expose the command only when the server advertises
   `supportsLobbyPlayerRemoval`, the local player is host, the target is an
   occupied non-host slot, and no launch or gameplay is active.
5. Name the target in a destructive confirmation. On confirmation, send one
   request and disable duplicate submission until an error or a newer lobby
   view arrives. Do not optimistically remove the roster row.
6. Let the host observe success through the authoritative
   `lobbyStateChanged` event. Close a player menu or confirmation if its target
   disappears from that view.
7. On a local `playerRemoved` event, atomically mark the membership terminal,
   disable reconnect, erase the resume token, cancel game preparation and file
   transfer, disconnect voice, clear lobby chat, and return to multiplayer
   home with `The host removed you from the lobby.`
8. Treat legacy `error.code == "removedFromLobby"` followed by socket close as
   the same terminal outcome. Do not route it through ordinary retry handling.
9. Keep connection loss and host removal distinct. An unexpected disconnect
   may resume; `removedByHost` never resumes the old membership.
10. Add exact JSON codec tests, capability-gating tests, command deduplication
    tests, terminal cleanup tests, and a regression test proving the old resume
    token cannot trigger a reconnect attempt.

For desktop v1, the TypeScript SDK owns wire codecs and terminal state, while
the desktop coordinator owns service cleanup and navigation. For desktop v2,
project `MultiplayerCommand::RemoveLobbyPlayer` through the Rust app-owned
lobby service; the Svelte `LobbyPlayerContextMenu` remains presentation-only
and dispatches the command after confirmation.

## Verification

Implemented relay coverage:

- host removes a connected guest;
- host removes a reconnecting guest;
- guest cannot remove anyone;
- host cannot remove itself;
- empty and invalid targets are rejected;
- stale epoch is rejected;
- pending launch and in-game removal are rejected;
- readiness and resume token are erased;
- target gets one terminal event and socket close;
- remaining clients get one authoritative roster update;
- old resume token cannot reconnect;
- removed subject may join again as a new membership;
- voice participant cleanup is attempted and failure is non-fatal.

Implemented Android/Kotlin coverage:

- exact JSON codecs in Rust and Kotlin;
- missing capability fields default false;
- host menu capability and state gating;
- confirmation cancel sends nothing;
- confirmation accept sends one command;
- local terminal event stops reconnect and voice;
- remote roster update removes the row without closing the host;
- legacy target fallback receives `removedFromLobby` before socket close.

The relay passes 234 unit tests and 5 smoke tests with clean Clippy output. The
voice service passes 26 tests with clean Clippy output. Android passes all 483
unit tests plus `lintDebug`. TypeScript codec and desktop runtime tests remain
part of desktop adoption.

Cross-platform acceptance matrix:

- Android host removes Android guest;
- Android host removes desktop guest;
- Desktop host removes Android guest;
- Desktop host removes desktop guest;
- connected voice and active chat close cleanly for the removed guest;
- reconnecting guest cannot reclaim the removed slot.

## Deployment Order

1. Deploy `ghcr.io/ccollier86/sb-webrtc:dfc3767` (or the matching `latest`)
   with the backward-compatible participant-removal endpoint.
2. Deploy `ghcr.io/ccollier86/sb-netplay-serv:4db189f` (or the matching
   `latest`) with the new protocol and server capability enabled.
3. Release Android and desktop clients with capability-gated UI.
4. Run the cross-platform acceptance matrix against production-like relay and
   voice services.

Server-first deployment is safe because old clients do not send the new
request. Client-first deployment is also safe because the action remains
hidden while `supportsLobbyPlayerRemoval` is missing or false.
