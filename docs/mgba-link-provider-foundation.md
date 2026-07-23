# mGBA link provider foundation

Status: implementation checkpoint, disabled by default.

The full cross-platform design is maintained with the Android client in
`sb-android-dolphin-gamecube/docs/link-cable-netplay-plan.md` and
`sb-android-dolphin-gamecube/docs/link-cable-implementation-slices.md`. This
document records the server invariants that must remain true while that design
is implemented.

## Product behavior

- A normal lobby becomes a two-player handheld link lobby after the host picks
  a supported Game Boy, Game Boy Color, or Game Boy Advance title.
- The guest selects their own supported handheld title. ROM identity does not
  need to match; game-level compatibility is left to the games themselves.
- Each emulator remains an ordinary local solo session. The server relays only
  real link traffic between the two mGBA endpoints.
- Leaving the lobby ends link transport after the reconnect grace period, but
  the local games continue running.
- Link play does not use controller lockstep, host snapshots, rollback, the
  controller frame clock, deterministic scheduled start, or coordinated pause.
- Pause-menu guidance is a client concern. Any future traffic-based pause-sync
  heuristic is a separate, opt-in slice and is not part of this provider.

## Safety boundary

Every room owns exactly one gameplay provider:

```text
GameplaySession
  |-- ControllerNetplaySession
  `-- LinkCableSession
```

Controller protocol v4/v5 remains the production path. A link provider must
fail closed when it receives controller input, snapshot, state-hash,
coordinated-pause, scheduled-start, recovery, or controller-frame operations.
The final link data plane must never enter controller event, input, recovery,
or debug buffers.

The current JSON `mode: "linkCable"` and `linkCablePacket` types are provisional
scaffolding. They are not the final SBLK wire contract and must not be
reinterpreted in place. Final link grants and traffic require:

- an authenticated, server-issued `roomScope`;
- explicit room, session, and cable epochs;
- a link capability/version gate;
- bounded per-room queues and pressure behavior;
- forwarding outside the registry-wide room lock;
- separate link control-plane and high-frequency data-plane events.

The provisional packet relay still publishes through the shared `RoomEvent`
channel, records every packet in the shared debug log, and forwards while the
registry-wide write lock is held. That known scaffold is covered by tests but
is an explicit production-enable blocker, not an acceptable rollout path.

## Rollout

`SB_NETPLAY_LINK_CABLE_ENABLED` gates creation of new link rooms and defaults to
`false`. When disabled, a valid link create request fails before registry room
construction with `linkCableUnavailable`. Controller rooms are never subject to
this gate.

Enabling the environment variable is not sufficient to ship the feature. It
may be enabled only after the final capability, SBLK, `roomScope`, per-room data
plane, reconnect, and cross-platform qualification slices pass.

## Required regression gates

Before every link-provider server checkpoint:

1. Controller v4/v5 golden JSON remains byte-for-byte unchanged.
2. Existing controller room, input, snapshot, pause, recovery, and WebSocket
   smoke suites pass.
3. Link creation is rejected by default and accepted only with explicit test
   opt-in.
4. Provider-selection tests prove that a room cannot own both providers.
5. Link-specific load tests prove that one saturated room cannot hold the
   registry-wide lock or consume another room's queue budget.

## Remaining server slices

1. Freeze controller v4/v5 goldens and the default-off provider boundary.
2. Replace provisional link JSON relay with the frozen SBLK contract.
3. Allocate and authorize `roomScope` plus room/session/cable epochs.
4. Add a bounded per-room link data plane with explicit overload and disconnect
   behavior.
5. Add reconnect/resume ownership and stale-epoch rejection.
6. Run two-client GB, GBC, and GBA qualification before enabling any rollout.

Future PSP and Nintendo 3DS multiplayer integrate as separate external-network
providers. PSP may initially allocate logical sessions on one shared service;
Nintendo 3DS may allocate one Docker container per session. Neither provider
changes or inherits controller-netplay or mGBA link packet semantics.
