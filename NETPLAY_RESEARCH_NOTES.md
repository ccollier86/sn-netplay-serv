# Netplay Research Notes

## Sources Reviewed

- RetroArch netplay docs: https://docs.libretro.com/development/retroarch/netplay/
- RetroArch netplay source folder: https://github.com/libretro/RetroArch/tree/master/network/netplay
- Dolphin netplay guide: https://dolphin-emu.dolphin-emu.org/docs/guides/netplay-guide/
- Dolphin netplay source:
  - https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/NetPlayServer.cpp
  - https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/NetPlayClient.cpp
- Mednafen netplay docs: https://mednafen.github.io/documentation/netplay.html
- GGPO SDK: https://github.com/pond3r/ggpo
- GGRS Rust rollback library: https://github.com/gschup/ggrs
- Matchbox Rust/WebRTC signaling: https://github.com/johanhelsing/matchbox

## What Applies To ShadowBoy

### RetroArch

RetroArch is the most useful conceptual reference for emulator netplay.

Relevant ideas:

- Netplay depends on deterministic cores, matching content, and matching input
  devices.
- All players must agree on frame numbers.
- Input is frame-numbered and sent every frame.
- The server is canonical for player slots and synchronization events.
- Initial join includes a canonical frame count and serialized state.
- Spectators are modeled as connections without input.
- Rollback requires a ring buffer of serialized states plus local/remote input.

ShadowBoy takeaway:

- Use strict `playerIndex` and `frame` validation from day one.
- Server should reject out-of-order or impossible frame input.
- The desktop runner needs a canonical frame counter before real netplay works.
- Save-state sync is not optional; it is how a guest joins the host's current
  gameplay state.
- Even if MVP is lockstep, design the protocol so rollback can be added later.

### Dolphin

Dolphin is the most useful UX reference for invite-code setup and host-managed
session state.

Relevant ideas:

- Users can host through direct connection or a traversal server.
- Traversal hosting produces a code that players can enter.
- Host is responsible for netplay setup, player list, controller assignment,
  buffer configuration, and starting the game.
- Dolphin synchronizes game identity, save data, and cheat/code state before
  starting.
- Input buffering is a user-visible netplay setting.
- Pad mappings are server-managed and broadcast to clients.

ShadowBoy takeaway:

- Our `Play Together` invite-code UX is proven.
- ShadowBoy should hide manual networking and always use the public relay for MVP.
- Host should control start/resync.
- Server must own slot assignment and broadcast room state.
- Compatibility must include game hash, core id/version, settings hash, and any
  netplay-relevant save/system data.

### Mednafen

Mednafen is the closest reference for standalone server-based emulator netplay.

Relevant ideas:

- Clients connect to a standalone server.
- Save states are used when connecting and when save states are loaded.
- Bandwidth matters for newer systems with large save states.
- Player/controller operations exist for swap/take/drop/list.
- Save-state transfer has security implications.

ShadowBoy takeaway:

- Keep save-state snapshot sizes capped.
- Treat save-state bytes as untrusted transfer data.
- Do not persist snapshots on the server.
- Add player-slot actions later, but keep MVP simple: host Player 1, guest
  Player 2.

### GGPO / GGRS

GGPO and GGRS are useful for future rollback, not the first server relay.

Relevant ideas:

- Rollback requires the client runtime to save state, load state, and advance one
  frame from a set of player inputs.
- GGRS has explicit session builders, player handles, input delay, spectators,
  sync tests, and desync detection.
- GGRS `SyncTestSession` is a good model for validating determinism locally.

ShadowBoy takeaway:

- Do not make rollback an MVP requirement.
- Build runner APIs around save/load/advance-frame so rollback stays possible.
- Add local deterministic sync tests before enabling rollback for any core.
- Consider GGRS patterns for future client-side rollback scheduling, but keep
  the server transport custom and ShadowBoy-specific.

### Matchbox

Matchbox is useful if we later want direct peer-to-peer WebRTC data channels.

Relevant ideas:

- Signaling server gets peers connected.
- After WebRTC negotiation, traffic can flow directly between peers.
- Supports low-latency game networking patterns and GGRS examples.

ShadowBoy takeaway:

- WebRTC/P2P is a later optimization.
- MVP should use WebSocket relay because it avoids NAT/ICE/TURN complexity and
  gives the simplest user experience.
- Keep the room protocol transport-agnostic enough that a direct data channel can
  eventually replace relayed input frames.

## Recommended ShadowBoy Direction

### MVP

- Rust `axum` WebSocket relay.
- License validation through the existing metadata/cheat service.
- Invite-code rooms.
- `maxPlayers = 2`, but use a `players: Vec<PlayerSlot>` model.
- Host is Player 1, guest is Player 2.
- Compatibility fingerprint exchange.
- Host save-state snapshot relay.
- Lockstep or small fixed input-delay input relay.
- Explicit pause/resync on mismatch or disconnect.

### Protocol Rules To Copy Conceptually

- Every gameplay input message includes:
  - `roomId`
  - `playerIndex`
  - `frame`
  - compact input payload
- Server validates:
  - connection owns `playerIndex`
  - frame is not lower than last accepted frame
  - frame is not wildly ahead of the room frame
  - room is in `playing` state
- Server broadcasts input with the authoritative `playerIndex`.
- Server broadcasts room state whenever player status changes.
- Host-controlled events such as start, pause, resync, and end happen at a
  canonical frame.

### Things Not To Copy For MVP

- RetroArch rollback implementation complexity.
- Dolphin direct-connect/traversal split.
- Mednafen command-console UX.
- P2P WebRTC negotiation.
- Spectators.
- 3-4 player UI.
- Player slot swapping.

## Open Engineering Questions

- Which ShadowBoy cores can provide deterministic frame stepping and stable
  save/load behavior?
- Can the runner expose a reliable frame counter for every supported core?
- How large are save-state snapshots for each target system?
- Do hardware-rendered cores produce deterministic enough state for netplay?
- Should MVP start with only 2D/software-render cores before N64/GameCube?
- How often can we compute state hashes without hurting performance?
