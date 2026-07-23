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

The JSON `mode: "linkCable"` control-plane shape is admitted only with the
explicit completed-contract version. The existing JSON `linkCablePacket`
envelope carries a frozen, language-neutral SBLK v1 frame with canonical
GB/GBC and GBA fixtures. The server data plane now provides:

- an authenticated per-player `linkCableGrant` with stable server-issued
  `roomScope` serialized as an exact decimal string, exact
  room/session/cable epochs, local slot, protocol, and bounded-queue limits;
- private `linkCableGrantUpdated` lifecycle messages for attach, abort,
  reconnect, and close;
- the explicit `linkContractVersion: 1` admission gate;
- two fixed-capacity queues owned by each link room, with no sender echo and no
  cross-room queue budget;
- exact slot, epoch, sequence-from-zero, SBLK namespace/body, and timestamp
  validation before forwarding;
- stateful GB/GBC and GBA transfer validation for mode readiness, exact
  transfer ids, sender phases, collision prevention, and committed data;
- fail-closed protocol, disconnect, provider, and queue-overflow behavior; and
- forwarding after the registry read lock has been released.

Link packets and grants never enter the shared `RoomEvent` channel, never
increment public `eventSeq`, and never enter room debug history. The control
WebSocket selects over the ordinary room channel and its authenticated
single-consumer link receiver independently.

## Rollout

`SB_NETPLAY_LINK_CABLE_ENABLED` gates creation of new link rooms and defaults to
`false`. When disabled, a valid link create request fails before registry room
construction with `linkCableUnavailable`. When enabled, link create and control
WebSocket admission require exact `linkContractVersion: 1`; missing or unknown
versions fail closed. Controller rooms are never subject to this gate.

Enabling the environment variable is not sufficient to ship the feature. It
may be enabled only after Android consumes the private grants, reconnect is
qualified through the native bridge, and the GB/GBC/GBA physical-device matrix
passes.

## Required regression gates

Before every link-provider server checkpoint:

1. Controller v4/v5 golden JSON remains byte-for-byte unchanged.
2. Existing controller room, input, snapshot, pause, recovery, and WebSocket
   smoke suites pass.
3. Link creation is rejected by default and accepted only with explicit test
   opt-in.
4. Provider-selection tests prove that a room cannot own both providers.
5. Link-specific tests prove that queue overflow clears only the owning room's
   queues, wakes both endpoints, and cannot mutate shared event/debug state.

## Remaining server slices

The server foundation now includes the private grant, frozen SBLK validation,
bounded targeted relay, endpoint ownership, stale-epoch rejection, and
reconnect generation changes. Remaining release work is:

1. Consume `linkCableGrant` / `linkCableGrantUpdated` in the Android transport
   actor and configure the native bridge with the exact granted identity.
2. Exercise disconnect/reconnect through two real Android processes and prove a
   strictly newer cable epoch is required before traffic resumes.
3. Run two-client GB, GBC, and GBA qualification before enabling any rollout.
4. Add multi-room saturation/load qualification before production capacity is
   raised.

Future PSP and Nintendo 3DS multiplayer integrate as separate external-network
providers. PSP may initially allocate logical sessions on one shared service;
Nintendo 3DS may allocate one Docker container per session. Neither provider
changes or inherits controller-netplay or mGBA link packet semantics.
