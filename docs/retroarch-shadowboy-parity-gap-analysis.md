# RetroArch To ShadowBoy Netplay Parity Gap Analysis

This document compares RetroArch's netplay architecture against the current
ShadowBoy relay, SDK, and desktop runner implementation. The goal is functional
parity, not a line-for-line copy. A difference matters when it can change frame
ownership, input timing, prediction, replay, desync repair, pause behavior, or
client recovery.

Primary RetroArch reference:

- `docs/retroarch-netplay-architecture.md`
- `/tmp/RetroArch-netplay-review/network/netplay/netplay_private.h`
- `/tmp/RetroArch-netplay-review/network/netplay/netplay_frontend.c`
- `/tmp/RetroArch-netplay-review/network/netplay/README`

Primary ShadowBoy reference:

- `src/rooms/room.rs`
- `src/rooms/room_controller_netplay_ops.rs`
- `src/rooms/room_frame_clock_ops.rs`
- `src/rooms/room_registry_sync_ops.rs`
- `src/rooms/room_state_hash_ops.rs`
- `src/rooms/room_adaptive_input_delay_ops.rs`
- `src/protocol/input_batch.rs`
- `sdk/kotlin/src/main/kotlin/app/shadowboy/netplay/sdk/state/ResyncState.kt`
- `../packages/netplay-sdk/src/state/frameClock.ts`
- `../packages/netplay-sdk/src/state/resync.ts`
- `../apps/desktop/src/main/netplay/DesktopNetplaySocketMessageHandler.ts`
- `../apps/desktop/src/main/netplay/DesktopNetplaySnapshotTransferService.ts`
- `../native/runner/src/netplay/runtime_session.rs`
- `../native/runner/src/cores/libretro_runtime.rs`

## Executive Summary

ShadowBoy is closest to RetroArch in the desktop runner's rollback/prediction
loop and in the relay's current contiguous input acceptance. The largest
remaining functional differences are not syntax or transport differences; they
are protocol-state differences:

1. RetroArch repairs drift with `LOAD_SAVESTATE(frame, bytes)` at an exact
   canonical frame. ShadowBoy repairs by resetting room sync, bumping the
   session epoch, sending a snapshot with no frame, and restarting netplay from
   `startFrame = 0`.
2. RetroArch's server is the frame clock because it advances its own frame
   stream and sends `INPUT` or `NOINPUT` for each frame. ShadowBoy has a
   headless 60Hz relay clock gated by host input.
3. RetroArch hashes exact frame state and either checks the exact frame or
   stores the remote CRC until that frame is reached. ShadowBoy compares exact
   same-frame hashes, but also treats nearby-frame matches as non-errors and
   waits for multiple true mismatches before repair.
4. RetroArch's input latency adaptation is local, bounded, and based on replay
   runtime. ShadowBoy chooses a relay-wide startup delay from network samples
   and then freezes runtime changes.
5. RetroArch's device model is broad and frame-scheduled. ShadowBoy uses fixed
   player slots and a fixed 10-byte standard-controller payload.
6. ShadowBoy has product-specific room auth, invite codes, resume tokens,
   epochs, telemetry, and dual WebSockets. Those are acceptable if they preserve
   the same frame semantics, but they currently hide some non-equivalent repair
   behavior.

## Difference Inventory

| Area | RetroArch Behavior | ShadowBoy Behavior | Functional Difference | Parity Action |
| --- | --- | --- | --- | --- |
| Session topology | Direct netplay connection with optional server role. The server can be an emulator participant and owns the sync clock. | REST room creation, control WebSocket, input WebSocket, license auth, invite codes, resume tokens. Relay is headless. | Product topology differs. This is acceptable only if the relay still provides a strict canonical frame stream. | Keep ShadowBoy topology, but make protocol semantics match RetroArch where timing and repair matter. |
| Server frame clock source | Server advances one frame at a time and sends `INPUT` or `NOINPUT`. It never sends frame `n + 1` while still on `n`. | `room_frame_clock_task` releases one frame every 16,666,667ns when host input exists for that frame. | ShadowBoy's clock is wall-clock 60Hz, not the host emulator's actual frame cadence. This can differ for non-60Hz cores, PAL content, throttling, or host catch-up. | Either carry a room nominal frame rate and tick against it, or drive release from host input arrival while rate-limiting to one canonical frame at a time. Do not bulk-release. |
| Server frame release authority | Server's own frame stream is authoritative. Clients see `server_frame_count`. | Relay emits `ServerFrame { frame, canonical_frame }`; runner stores `server_released_frame`. | Close approximation after host-cursor fix, but `canonical_frame` is diagnostic and not a true RetroArch `server_frame_count` cursor in every client path. | Treat `serverFrame.frame` as the canonical release cursor everywhere. Avoid dual meanings between `frame`, `canonicalFrame`, and `roomFrame`. |
| Input acceptance | Per playing client: old input ignored, exact next frame accepted, future gaps rejected. | `accept_input_frame` now enforces `input.frame == next_input_frame_for_player`; old ignored; future rejected. | Mostly aligned. ShadowBoy also allows frames up to `room_frame + 240` as an abuse guard after contiguous acceptance. RetroArch's practical rollback window is 60 frames. | Keep contiguous rule. Consider reducing future acceptance to the rollback window plus latency margin once client behavior is stable. |
| Input batching | RetroArch commands are frame-command oriented. | `SBI1` can contain up to 4 frames per binary input batch. | Batching is fine if decoded in order and each frame still passes the contiguous cursor. It can obscure per-frame diagnostics if logged only as a batch. | Keep batching, but record per-frame acceptance/rejection in diagnostics and telemetry. |
| NOINPUT equivalent | Server sends explicit `NOINPUT` for frames where the server has no local input. | ShadowBoy emits `SBF1` `serverFrame` records. | Equivalent intent. Difference is that ShadowBoy's relay has no participant input of its own. | Keep `serverFrame` as the NOINPUT-style release tick; clients must not treat it as optional. |
| Frame buffer storage | Fixed ring size `NETPLAY_MAX_STALL_FRAMES + 2`, normally 62 records. | Desktop runner uses a `BTreeMap<u64, NetplayDeltaFrame>` and prunes confirmed frames. | Data structure is different but can be functionally equivalent. Risk is unbounded growth or retaining frames needed for hashes longer than the rollback window. | Enforce a hard retention cap around the rollback/hash windows and emit diagnostics if pruning would discard an unverified needed frame. |
| Frame cursors | RetroArch tracks `self`, `run`, `other`, `unread`, `read[client]`, `server`, and `replay` frame cursors. | Desktop runner tracks `input_frame`, `run_frame`, `synced_frame`, `unread_frame`, `read_frames[]`, and relay released frame. | Mostly aligned. ShadowBoy does not maintain a separate server cursor in the same shape; it clamps unread to relay release count. | Keep the mapping explicit in runner docs/tests. Test every cursor transition against RetroArch-style examples. |
| Local input latency | RetroArch captures local input ahead of run frame until `run + input_latency_frames <= self`. | Desktop runner captures local input through `run_frame + input_delay_frames`. | Aligned at the runner level. | Preserve this. Android must match this exact convention. |
| First-frame input | RetroArch first missing prior input effectively resolves to neutral/default. | Desktop runner `previous_real_input` falls back to default input. | Aligned. | Preserve. |
| Prediction source | Missing remote input is simulated from that player's previous real input. | Desktop runner uses previous real input for first simulation. | Aligned. | Preserve. |
| Resimulation buttons | RetroArch preserves direction duration and refreshes non-direction buttons from newest real input during resimulation. | Desktop runner preserves `RETROARCH_RESIMULATED_DIRECTION_BITS` and refreshes other joypad bits. | Aligned for digital joypad bits. | Preserve. |
| Resimulation analog axes | RetroArch has richer input-device data and analog handling through device input state. | ShadowBoy's fixed payload preserves left/right analog axes from previous simulation during resimulation. | Needs verification. This may not match RetroArch's exact analog behavior for analog devices. It is currently a functional difference for analog-heavy cores. | Audit RetroArch analog path specifically before declaring analog parity. Add tests for N64/GameCube analog prediction. |
| Replay trigger | RetroArch replays when real input replaces simulated input and resolved input changes. | Desktop runner `replay_request` returns the oldest frame where resolved input changed. | Mostly aligned. | Add parity tests for changed button, unchanged prediction, direction-held resim, and multiple remote frames arriving late. |
| Replay execution | RetroArch loads the saved state at the replay point, suppresses normal output during replay, re-runs to current frame, and updates runtime samples. | Desktop libretro runtime loads state for replay, suppresses AV output during replay, and advances to current frame. | Core replay behavior is aligned. ShadowBoy does not collect RetroArch's 120-sample frame runtime average during replay for latency tuning. | Keep replay suppression. Add frame-runtime sampling only if we choose RetroArch-equivalent input latency adaptation. |
| Running-fast stall | RetroArch stalls when local gets too far ahead of unread input, with a 60-frame window and 2-frame resume margin. | Desktop runner stalls when `run_frame - relayReleasedFrameCount >= 60`, resumes with a 2-frame margin. | Close equivalent, but the stall reference is relay release count, not unread remote input directly. | Accept if relay release count is the canonical unread cap. Test host and guest both running fast. |
| Catch-up enter | RetroArch enters catch-up only after being more than 3 frames behind and still falling behind after a 500ms probe. | Desktop runner uses 3-frame threshold and 500ms probe. | Aligned. | Preserve. |
| Catch-up exit | RetroArch exits when `self + 1 >= lo_frame_count`. | Desktop runner exits when `run_frame + 1 >= relayReleasedFrameCount`. | Close but not identical cursor choice. RetroArch uses self frame, not run frame. | Verify with input delay > 0. If behavior differs, use the RetroArch-equivalent local/self cursor. |
| Server-requested stall | RetroArch server can send `STALL` to clients that are ahead of the slower peer, rate limited by `NETPLAY_MAX_REQ_STALL_FREQUENCY`. | ShadowBoy has no explicit server-requested stall command. Clients self-stall against relay release count. | Functional difference. The relay clock may indirectly limit fast clients, but cannot tell only one ahead client to stall for a bounded number of frames. | Add an explicit stall request only if self-stall is insufficient in real sessions. Keep it behind diagnostics. |
| Stall timeout while paused | RetroArch suppresses stall timeout while the peer is paused. | ShadowBoy has heartbeats/reconnect state and synchronized pause holders. Timeout behavior is not documented as "suppressed while peer paused." | Potential difference during long pause menus or backgrounding. | Verify heartbeat continues while paused or explicitly exempt paused rooms from stall/recovery teardown. |
| Input latency adaptation | RetroArch adjusts `input_latency_frames` locally within host-provided min/range using a 120-frame runtime average. Defaults min/range are 0/0. | ShadowBoy selects an initial room delay from network samples, clamps 1..8, and disables runtime changes. | Not equivalent. ShadowBoy delay is network-derived and relay-wide; RetroArch delay is local and runtime-derived within configured bounds. | Decide intentionally. For parity, implement local bounded runtime latency in clients and stop treating relay delay as adaptive beyond initial host contract. |
| State hash algorithm | RetroArch uses CRC-32 of serialized state. | ShadowBoy uses SHA-256 of serialized state. | Hash algorithm differs but correctness is not harmed. SHA-256 is heavier. | Keep SHA-256 if telemetry shows cost is acceptable. The key parity issue is exact-frame semantics, not algorithm choice. |
| Hash cadence | RetroArch default check cadence is 600 frames. | Desktop SDK reports every 60 frames by default. Server retains 600 frames. | More frequent hashing can cost CPU/memory, especially on large Android states. It also makes drift events much noisier. | Consider reverting default cadence to 600 frames, or make 60 a diagnostic-only build setting. |
| Hash frame semantics | RetroArch checks the exact frame if present; otherwise stores remote CRC in that frame and checks when caught up. | ShadowBoy compares only after connected players reported the same frame. Nearby-frame matches are diagnostic and reset mismatch streak. | Difference. Nearby match reset can hide real deterministic drift if clients are regularly offset. RetroArch does not treat nearby equality as exact sync. | Nearby matches should be diagnostics only. Repair decisions should be based on exact canonical frame mismatches or an explicit offset-correction policy. |
| Hash mismatch threshold | RetroArch requests savestate after valid CRC mismatch, with special first-check invalid-CRC handling. | ShadowBoy requires 3 true mismatches before resync. | ShadowBoy repairs later. This can let desync become visible for longer. | If parity is the goal, request repair after the first confirmed exact-frame mismatch. Keep multi-mismatch threshold only if telemetry proves false positives. |
| Hash first-failure handling | RetroArch can mark CRC invalid for a core/session if the first check fails. | ShadowBoy has no equivalent "hashes invalid for this core/session" state. | Potential functional difference for cores whose save states are not deterministic across clients. | Add per-session "hash validation failed" state or gate unsupported cores/platform pairs before starting. |
| State repair request | RetroArch client sends `REQUEST_SAVESTATE`; server delays sending until the next pre-frame. | ShadowBoy server emits `stateHashMismatch`, resets sync state, and clients restart compatibility/snapshot/ready flow. | Major difference. ShadowBoy repair is room lifecycle repair, not frame repair. | Add explicit `requestSavestate` / `loadSavestate` or equivalent messages with load-frame semantics. |
| State repair payload | RetroArch sends `LOAD_SAVESTATE(frame, uncompressed_size, bytes)`. | ShadowBoy snapshot manifest has only `totalBytes` and `sha256`. | Major difference. Snapshot bytes are not tied to the canonical frame they represent. | Add `frame` or `loadFrame` to snapshot manifest and every repair event. |
| State repair load behavior | RetroArch requires load frame to equal `server_frame_count`, writes state into the ring at that frame, forces rewind, and advances stale read cursors. | Desktop guest writes the snapshot to disk, calls runner `loadStateFromFile`, sends ready, then server emits `startSession(0)`. | Major difference. ShadowBoy discards old frame continuity and starts a new epoch from frame 0. | Add runner support for loading repair state at a target frame and rebuilding cursors, or formally define repair as a hard new session and stop calling it RetroArch parity. |
| Start frame | RetroArch `SYNC` includes the current frame; load-state repair preserves frame. | ShadowBoy `mark_ready_impl` always emits `startSession` with `start_frame = 0`. | Different for repair/reconnect. Initial session can start at 0, but repair should not. | Emit the canonical repair frame when resyncing an active room. |
| Snapshot compression/protocol | RetroArch supports compression negotiation and state-size metadata. | ShadowBoy chunks raw bytes with sha256 and max 64MiB. | Compression is not required for correctness. Missing frame is the real gap. | Optional compression later; prioritize load-frame semantics. |
| Save-state quirks | RetroArch handles serialization quirks like initialization, endian/platform dependence, and state-size growth. | ShadowBoy relies on core wrappers and stateFormat compatibility. | Potential difference for cross-platform Android/Desktop or cores with platform-dependent states. | Keep strict `stateFormat` gates. Add compatibility tests per core/platform pair before enabling cross-platform. |
| Pause protocol | RetroArch `PAUSE`/`RESUME` mark peer pause state and local pause; pause suppresses stall timeout. | ShadowBoy has scheduled pause frames, holders, idempotency keys, acknowledgements, and resume-at-frame. | Product behavior is more structured and can be better, but it is not RetroArch's simple pause model. It must still pause both clients at the same frame. | Keep scheduled pause, but verify timeouts and input acceptance while paused. Add tests for host-pause, guest-pause, double-pause, reconnect while paused. |
| Pause input window | RetroArch pauses runtime state without accepting unbounded future input. | ShadowBoy accepts frames through `pause_at_frame + inputDelayFrames` and ignores beyond. | Different but reasonable. Risk is silently ignored input creating cursor expectations after resume. | Ensure ignored post-pause input is resent or not marked sent locally after resume. |
| Reconnect | RetroArch supports join/leave/mode changes scheduled on future frames. | ShadowBoy has resume tokens, room/session epochs, recovery states, and may require resync. | Product-specific and acceptable. It is not RetroArch-equivalent because reconnect generally restarts from snapshot rather than preserving continuous frame cursors. | Make recovery either exact-frame load repair or clearly a new session with new frame base. |
| Room/session epochs | RetroArch does not use ShadowBoy-style room/session epochs. | ShadowBoy uses epochs to reject stale messages. | Product-specific. Good for safety. | Keep epochs, but include them on every snapshot/repair packet and do not treat missing epochs as fatal during valid legacy initial sync only. |
| Input socket lag | RetroArch uses reliable command stream semantics; delayed data is still processed in order. | ShadowBoy input channel uses WebSocket plus broadcast receivers; lagged receiver closes. | WebSocket is reliable TCP, but broadcast lag causes local disconnect rather than backpressure. | Keep close-on-lag for safety, but record diagnostics and ensure recovery does not drop into single-player. |
| Handshake compatibility | RetroArch exchanges content CRC, core name/version, devices, mappings, SRAM, and settings. | ShadowBoy uses session descriptors, content hash, stateFormat/coreVersion, settingsHash, cheatsHash, systemDataHash, and host snapshot. | Different but acceptable. State compatibility must be strict enough that save-state bytes load correctly. | Do not use informational core build as a hard gate. Use `stateFormat` as the hard save-state compatibility value. |
| SRAM transfer | RetroArch `SYNC` includes SRAM. | ShadowBoy starts from host snapshot and does not have a separate SRAM sync phase. | Initial gameplay can still be equivalent if snapshot fully captures SRAM/relevant memory. | Confirm every enabled core's netplay snapshot captures SRAM or save RAM state. |
| Device model | RetroArch supports up to 16 input devices, keyboard pseudo-device, multiple clients, share modes, slave/spectator, and rich input payload sizes. | ShadowBoy supports fixed MVP player slots and fixed 10-byte standard controller payloads. | Intentional product scope. Not full RetroArch feature parity. Can affect games requiring special devices or shared-controller behavior. | Keep scoped to supported controller netplay. Do not claim parity for unsupported device modes. |
| Player/mode changes | RetroArch schedules mode changes against future frame numbers. | ShadowBoy assigns host Player 1, guest Player 2 and uses room lifecycle/status changes. | Acceptable for current two-player MVP, but not equivalent for future 3/4-player, spectators, or controller sharing. | When expanding players, schedule membership changes against future canonical frames. |
| Core link packets | RetroArch has `NETPACKET` for raw packet core interfaces. | ShadowBoy has a separate `linkCable` mode and packet relay. | Different architecture. Fine if kept separate from controller netplay. | Do not mix link-cable compatibility with controller-netplay state sync. |
| Runtime settings | RetroArch sends settings like allow pausing and input latency frames. | ShadowBoy hides input delay UI and uses room controller settings. | Product-specific. Input delay difference still matters for parity. | If host options affect sync, include them in room contract but not in deterministic settings hash unless they change core state. |
| Cheats | RetroArch has a `CHEATS` command but it is marked unsupported. | ShadowBoy hashes cheats for compatibility. | ShadowBoy is stricter. This is acceptable because mismatched cheats can desync. | Preserve strict cheat gating unless cheat sync is implemented. |
| Telemetry | RetroArch has diagnostics but not ShadowBoy's Postgres telemetry model. | ShadowBoy drains runtime/server events to telemetry. | Product-specific and useful. Must not be in the hot input path. | Keep async/bounded. Use it to tune, not to drive critical frame decisions without tests. |
| SDK role | RetroArch netplay logic lives in the frontend/runtime. | ShadowBoy SDKs own wire types, heartbeat, reconnect, diagnostics, and state helpers; runners own rollback. | Acceptable split. Risk is client teams assuming SDK implements prediction/rollback. | SDK docs must say rollback is a runner responsibility and list required runner hooks. |
| TS/Kotlin resync phase order | Not applicable to RetroArch. | TypeScript `markSnapshotSendComplete`/`markSnapshotLoadComplete` go to `waitingForReady`; Kotlin goes to `WaitingForCompatibility`. | ShadowBoy internal inconsistency. Can cause Desktop and Android to sequence repair differently. | Normalize both SDKs before declaring protocol stable. |
| Documentation accuracy | RetroArch docs are source of truth. | Some ShadowBoy handoff docs are stale; for example one Android handoff still describes waiting for every player's input before server frame release. | Stale docs can lead Android/Desktop to implement different runtime semantics. | Update handoff docs after each protocol parity change and point both clients at the same parity document. |

## Current Parity Classification

### Functionally Close

- Contiguous per-player input acceptance.
- Host-gated one-frame relay release after the latest server fix.
- Desktop runner rollback, replay, and digital joypad prediction.
- Running-fast stall window and catch-up probe shape.
- Local input capture through `runFrame + inputDelayFrames`.
- AV suppression during replay.

### Not Yet Functionally Equivalent

- Exact-frame savestate repair.
- Hash mismatch repair timing and nearby-frame handling.
- Relay clock source for variable frame-rate cores.
- Input latency adaptation.
- Server-requested stall.
- Android/Desktop SDK resync phase consistency.
- Analog-device prediction parity.
- Pause timeout and recovery behavior while peer-paused.

### Intentional Product Divergence

- Invite codes, license auth, resume tokens, room epochs, and server telemetry.
- Fixed ShadowBoy player slots instead of RetroArch's full device-sharing matrix.
- Separate link-cable relay mode.
- SHA-256 instead of CRC-32, if performance stays acceptable.
- Dual WebSockets, if the binary input channel preserves ordering and recovery.

## Highest-Impact Parity Work

1. Add exact-frame repair messages.
   The protocol needs a canonical repair frame on the request, snapshot manifest,
   snapshot chunks, and completion event. The receiver must reject a repair state
   for the wrong epoch or wrong frame.

2. Change active resync from "restart from frame 0" to "load frame N and replay."
   The runner needs a repair entry point that loads the received state into the
   frame buffer at `loadFrame`, advances stale read cursors, marks force-replay,
   and resumes from the canonical frame.

3. Make nearby-frame hash matches diagnostic-only.
   They are useful for measuring frame skew, but they should not reset the true
   mismatch streak unless we intentionally implement an offset-correction policy.

4. Decide the frame-clock source.
   If the relay remains headless, the safest parity model is a host-input-driven
   canonical stream with one-frame release and an explicit room frame rate. A
   hard-coded 60Hz clock is not equivalent for every system.

5. Normalize SDK resync state machines.
   TypeScript and Kotlin must transition through the same phases for
   compatibility, snapshot load/send, ready, and start.

6. Verify analog prediction against RetroArch.
   The digital behavior is close. Analog behavior should not be called parity
   until tests show it behaves the same for N64/GameCube-style axes.

7. Update Android handoff docs after the protocol decision.
   Android must implement the same cursor names and meanings as desktop. Matching
   WebSocket messages is not enough if the runner advances frames differently.

## Functional Parity Target

The ShadowBoy version should satisfy these invariants before we call it
RetroArch-equivalent for controller netplay:

1. Every accepted input frame is contiguous per player.
2. The relay releases exactly one canonical frame at a time.
3. Clients may run ahead only within the rollback/stall window.
4. Missing remote input is predicted from previous real input.
5. When real input differs, clients load the last safe state and replay forward.
6. State hashes are compared against the exact same emulated frame.
7. On confirmed drift, the host sends a state explicitly tied to a canonical
   load frame.
8. Guests load that state at that frame, not as an unframed session restart.
9. Pause/resume is synchronized runtime state, not only UI state.
10. Desktop and Android use the same cursor definitions and recovery flow.

