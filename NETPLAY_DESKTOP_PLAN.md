# ShadowBoy Desktop Netplay Plan

## Goal

Add dead-simple two-player netplay to ShadowBoy Desktop while keeping normal
single-player library state safe. The desktop app should handle room creation,
invite-code joining, controller assignment, save-state sync, input relay, and
netplay-specific resume data without requiring LAN setup or manual networking.

## Hard State Safety Rule

Netplay must never overwrite normal single-player autosaves, suspend states,
SRAM, memory cards, or managed save states unless the user explicitly chooses to
merge or keep netplay data.

## Desktop Surfaces

- Add a `Play Together` action from the game detail view and active game view.
- Host flow:
  - Validate the user's ShadowBoy license.
  - Create a netplay room.
  - Show invite code.
  - Show Player 1 as host and Player 2 as waiting.
- Guest flow:
  - Enter invite code.
  - Validate the user's ShadowBoy license.
  - Join as Player 2.
  - Show compatibility and sync progress.
- Add room status UI:
  - connecting
  - checking compatibility
  - syncing state
  - ready
  - playing
  - disconnected
  - resync required
- Add lightweight in-game status/toasts:
  - invite created
  - guest joined
  - syncing netplay state
  - session started
  - connection interrupted
  - resyncing
  - session ended

## Player Slots

- MVP supports two players.
- Host is Player 1.
- Guest is Player 2.
- Desktop should render slots from the server's `players` array instead of
  hardcoding host/guest fields.
- The server is authoritative for `playerIndex`.
- Desktop must reject local attempts to send input for any slot it does not own.
- The data model should be ready for future three-player or four-player rooms.

## Compatibility Fingerprint

Before starting, both clients must exchange and compare a netplay compatibility
fingerprint.

Fingerprint fields:

- ShadowBoy desktop version.
- Netplay protocol version.
- System id.
- Core id.
- Core version or core build hash.
- ROM content hash.
- Disc/content layout hash where applicable.
- Netplay-relevant emulator settings hash.
- Cheat/code hash.
- BIOS/system-data hash if the core requires it.
- Save-data mode identifier.

If fingerprints do not match, the session must not start. Desktop should show a
clear reason when possible, for example `Different ROM`, `Different core`, or
`Different cheats`.

## Netplay Save-State Sync

- Host creates a temporary save-state snapshot when the guest is ready.
- Snapshot is chunked and relayed through the server.
- Guest validates snapshot size and checksum before loading.
- Snapshot bytes are treated as untrusted input.
- Snapshot is loaded into a netplay runtime context, not the user's normal
  single-player resume slot.
- Server must not persist snapshot bytes.

## Netplay Autosave Namespace

Netplay needs its own autosave/suspend namespace.

Normal single-player autosave remains unchanged:

```text
single-player autosave
single-player SRAM / memory card
single-player managed save states
```

Netplay uses separate managed data:

```text
netplay autosave
netplay SRAM / memory card sandbox
netplay temporary sync snapshot
netplay managed save-state exports
```

Recommended key shape:

```text
gameId
systemId
coreId
romContentHash
netplayRoomId or netplaySessionId
playerIndex
saveDataMode = "netplay"
```

Behavior:

- Starting or joining netplay must not load the normal single-player autosave
  unless the user explicitly chooses to host from current solo state.
- Host sync snapshots should first land in a temporary netplay snapshot path.
- Once the session is stable, Desktop may write a `Netplay Autosave` entry.
- Netplay autosaves should be resumable only from netplay flows.
- Netplay autosaves should appear separately from normal autosaves if exposed in
  the save-state library.
- A permanent save from netplay should enter the managed save-state library with
  a `Netplay` tag or badge.
- The user must have an explicit action before netplay SRAM or memory-card data
  replaces normal single-player save data.

## SRAM And Memory Card Policy

- Use sandboxed netplay save-data paths during sessions.
- Never write guest-provided SRAM or memory-card data into the normal solo path.
- For host sessions, copy solo save data into the netplay sandbox only when the
  host chooses to start from current progress.
- After a session, offer explicit actions:
  - discard netplay save data
  - keep as netplay data
  - export to save-state library
  - replace single-player save data
- `replace single-player save data` must require confirmation.

## Input Runtime Requirements

- Desktop runner needs a canonical netplay frame counter.
- Every input packet sent to the server includes:
  - room id
  - player index
  - frame
  - compact input payload
- Input should be sampled once per emulation frame.
- The runner should support a small fixed input delay for MVP.
- If required remote input is missing for a frame, the runner should pause or
  wait rather than simulate unsafe input for MVP.
- Future rollback should be possible, so the runtime should avoid designs that
  prevent save/load/advance-one-frame scheduling later.

## Controller Behavior

- Player 1 uses the host's selected controller/profile.
- Player 2 uses the guest's selected controller/profile.
- Custom per-core controller profiles still apply locally.
- Server only relays normalized input payloads, not physical device names.
- Desktop should show which local controller is assigned to the local player.

## Session Lifecycle

1. Host launches game.
2. Host opens `Play Together`.
3. Desktop creates room and receives invite code.
4. Guest enters invite code.
5. Server assigns guest to Player 2.
6. Both clients send compatibility fingerprints.
7. Host creates temporary netplay snapshot.
8. Guest receives and validates snapshot.
9. Both clients enter ready state.
10. Server sends start frame.
11. Clients relay frame-numbered input.
12. On disconnect or mismatch, session pauses and can resync or end.
13. On end, Desktop keeps netplay data separate unless the user explicitly
    chooses otherwise.

## Desktop Implementation Areas

- Shared netplay protocol types.
- Main process netplay service.
- Renderer room/invite UI.
- Active game netplay overlay/status.
- Runner frame counter and input capture hooks.
- Save-state snapshot chunking.
- Netplay-specific save-data path resolver.
- Netplay autosave metadata.
- Compatibility fingerprint builder.
- License validation client call through the netplay server flow.
- Tests for state isolation and path safety.

## MVP Acceptance Checks

- Creating an invite code does not touch single-player autosaves.
- Joining a room does not touch single-player autosaves.
- Host snapshot sync writes only to netplay temp paths.
- Netplay autosave creates a distinct `Netplay Autosave` entry.
- Ending a netplay session does not overwrite solo SRAM or memory-card data.
- Replacing solo save data requires an explicit confirmation.
- Player 1 and Player 2 cannot send input for each other's slot.
- Mismatched ROM/core/settings fingerprints block start.
- Disconnect pauses or ends the session without corrupting solo state.
