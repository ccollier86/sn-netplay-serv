# Lobby Player Removal Contract

Status: proposed implementation contract

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

## Verified Starting Point

As of relay commit `bfe81a4`:

- The relay has no host player-removal operation or wire message.
- Android has no player-removal SDK command.
- Desktop v1 has no connected lobby-removal command.
- Desktop v2 already has a `Remove from lobby` presentation item and computes
  `can_kick`, but no runtime command reaches the relay.
- Normal guest leave already contains most of the required state cleanup:
  clearing the slot and resume token, removing readiness, cancelling the
  pending lobby launch, and returning the lobby to setup state.

The desktop v2 presentation must remain capability-gated until this contract
is implemented. A visible action that cannot reach the relay is a bug.

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
  "lobbyEpoch": 7,
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

The server sends the targeted terminal event before the normal
`lobbyStateChanged` broadcast for the same post-removal state. The removed
session sends the terminal event and exits its WebSocket loop. Other sessions
ignore the targeted domain event and then consume the roster update.

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

- Add trusted participant-removal request routing and API documentation.
- Extend `VoiceProvider` and `LiveKitVoiceProvider` with participant removal.
- Add service validation, telemetry, metrics, and provider tests.

### Android / Kotlin SDK

- `sdk/kotlin/.../protocol/LobbyMessages.kt`: request and event.
- `sdk/kotlin/.../protocol/Lobbies.kt`: capability fields and reason enum.
- `sdk/kotlin/.../state/LobbyStateMachine.kt`: terminal removed state.
- Android lobby coordinator: `removePlayer(playerIndex)` command and terminal
  cleanup.
- v2 lobby mapper: expose removal only for host, guest target, server support,
  and idle launch state.
- player menu: destructive confirmation followed by the real command.

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

## Required Tests

Relay tests:

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

SDK/client tests:

- exact JSON codecs in Rust, Kotlin, and TypeScript;
- missing capability fields default false;
- host menu capability and state gating;
- confirmation cancel sends nothing;
- confirmation accept sends one command;
- local terminal event stops reconnect and voice;
- remote roster update removes the row without closing the host;
- legacy target fallback receives `removedFromLobby` before socket close.

Cross-platform acceptance matrix:

- Android host removes Android guest;
- Android host removes desktop guest;
- Desktop host removes Android guest;
- Desktop host removes desktop guest;
- connected voice and active chat close cleanly for the removed guest;
- reconnecting guest cannot reclaim the removed slot.

## Deployment Order

1. Deploy the backward-compatible `sb-webrtc` participant-removal endpoint.
2. Deploy the relay with the new protocol and server capability enabled.
3. Release Android and desktop clients with capability-gated UI.
4. Run the cross-platform acceptance matrix against production-like relay and
   voice services.

Server-first deployment is safe because old clients do not send the new
request. Client-first deployment is also safe because the action remains
hidden while `supportsLobbyPlayerRemoval` is missing or false.
