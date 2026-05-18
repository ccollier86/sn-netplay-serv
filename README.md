# ShadowBoy Netplay Server

Rust relay server for ShadowBoy Desktop netplay. The server coordinates invite
codes, license verification, player slots, compatibility checks, state sync, and
frame-numbered input relay. It never runs emulator cores, never receives ROMs,
and never streams gameplay video.

## Current Slice

Implemented:

- `GET /health`
- `POST /v1/rooms` with protocol version and game/core descriptor
- `GET /v1/rooms/{invite_code}/status`
- `GET /v1/ws?inviteCode=AB23-CD&role=host&protocolVersion=3`
- `GET /v1/ws?inviteCode=AB23-CD&role=guest&protocolVersion=3`
- `GET /v1/ws?inviteCode=AB23-CD&protocolVersion=3&playerIndex=1&roomEpoch=2&resumeToken=...`
- `GET /internal/metrics`
- `GET /internal/rooms`
- `GET /internal/rooms/{invite_code}`
- `GET /internal/rooms/{invite_code}/events`
- `GET /internal/recent-events`
- HTTP-backed license-authority client
- server-side netplay entitlement authorization through the metadata service
- fakeable `LicenseAuthority` trait
- configurable per-action rate limits
- authenticated internal admin endpoints
- active-room observability snapshots
- production JSON logging option
- Docker/Coolify deployment artifacts
- GHCR build/push scripts
- in-memory room registry
- invite preview descriptors for local ROM/core matching
- request-body hashing for protected create-room signatures
- room WebSocket upgrade path
- host socket attachment to Player 1
- guest socket join as Player 2
- room state broadcasts over WebSocket
- basic WebSocket `ping` / `pong`
- compatibility fingerprint handling
- ready/start handling
- host snapshot chunk and manifest relay
- frame-numbered input validation and relay
- protocol v3 room/session epochs
- resume tokens for slot reclaim after disconnect
- app-level heartbeat acknowledgement
- heartbeat timeout recovery and reconnect grace cleanup
- coordinated pause/resume for in-game menu and lifecycle pauses
- sanitized per-room and recent event logs
- abandoned waiting-room expiration
- invite-code parsing and generation
- Player 1 / Player 2 slot model
- compatibility fingerprint comparison
- input-frame ownership and frame-bound validation
- snapshot chunk and checksum validation helpers
- unit and route tests

Not implemented yet:

- latency display and jitter quality scoring
- durable room storage across relay restarts
- multi-instance shared room backend

## Required Environment

```text
SB_NETPLAY_BIND_ADDR=127.0.0.1:8077
SB_NETPLAY_AUTHORIZE_URL=https://assets.shadowboy.app/internal/netplay/authorize
SB_NETPLAY_LICENSE_INTERNAL_SECRET=<server-to-server-secret>
SB_NETPLAY_ADMIN_TOKEN=<long-random-operator-token>
SB_NETPLAY_LOG_FORMAT=json
SB_NETPLAY_TRUST_PROXY_HEADERS=true
SB_NETPLAY_RATE_CREATE_ROOM_PER_MINUTE=12
SB_NETPLAY_RATE_WS_JOIN_PER_MINUTE=30
SB_NETPLAY_RATE_ROOM_STATUS_PER_MINUTE=120
SB_NETPLAY_RECONNECT_GRACE_SECONDS=90
SB_NETPLAY_HEARTBEAT_STALE_SECONDS=15
SB_NETPLAY_HEARTBEAT_DISCONNECT_SECONDS=30
SB_NETPLAY_ROOM_IDLE_SECONDS=300
```

`SB_NETPLAY_BIND_ADDR` is optional and defaults to `127.0.0.1:8077`.
`SB_NETPLAY_DESKTOP_AUTHORIZE_URL` and `SB_NETPLAY_LICENSE_VERIFY_URL` are still
accepted as legacy fallbacks for the authorization URL.

For Coolify, set `SB_NETPLAY_BIND_ADDR=0.0.0.0:8077` and let Coolify own the
public reverse proxy and TLS certificate. Deploy with
`docker-compose.coolify.yml`, using an image from GHCR. See `deploy/coolify.md`
and `deploy/ghcr.md`.

## Remote Operations

The server does not need a database or object storage for the MVP. It is a
stateless relay with in-memory rooms. Run a single instance until we add sticky
room routing or shared state.

The server also does not sync ROM files. It stores and returns game/core
descriptors so Desktop can find a matching local ROM by hash or guide the user
to import their own copy.

Remote debugging endpoints:

```bash
curl -fsS https://netplay.shadowboy.app/health
curl -fsS -H "Authorization: Bearer $SB_NETPLAY_ADMIN_TOKEN" \
  https://netplay.shadowboy.app/internal/metrics
curl -fsS -H "Authorization: Bearer $SB_NETPLAY_ADMIN_TOKEN" \
  https://netplay.shadowboy.app/internal/rooms
curl -fsS -H "Authorization: Bearer $SB_NETPLAY_ADMIN_TOKEN" \
  "https://netplay.shadowboy.app/internal/rooms/AB23-CD/events?limit=100"
curl -fsS -H "Authorization: Bearer $SB_NETPLAY_ADMIN_TOKEN" \
  "https://netplay.shadowboy.app/internal/recent-events?limit=100"
```

## Client Authorization

Room creation requires the same protected-client identity ShadowBoy clients
already use for metadata, cheats, billing, and updates:

```text
Authorization: Bearer <client access token>
X-Client-Kind: desktop | android
X-Install-Id: <installationId>
X-Req-Ts: <epoch milliseconds>
X-Req-Nonce: <unique nonce>
X-Req-Sig: <base64 signature>
```

`X-Client-Kind` is optional for existing Desktop clients and defaults to
`desktop`. Android clients must send `X-Client-Kind: android`. The relay also
accepts `X-Installation-Id` as an install-id alias for Android.

The relay sends the access token, client kind, install id, requested feature
`netplay`, and the signed netplay request proof to the metadata service using
`SB_NETPLAY_LICENSE_INTERNAL_SECRET`.

The metadata service should validate:

- the client access token
- the install id
- the protected request signature when signature fields are present
- nonce/timestamp freshness
- app/version policy tied to the token

Desktop authorization asks for `requiredEntitlement: "premiumOrTrial"`. Android
authorization asks for `requiredEntitlement: "eligibleClient"` so the Android
session, install key, signature, nonce, timestamp, and Play Integrity-backed
bootstrap can be validated without duplicating app-side premium gating.

Accepted response shapes:

- explicit authorization, for example `{ "authorized": true }`
- feature entitlement, for example `{ "features": { "netplay": true } }`
- premium/trial account state, for example `{ "accessStatus": "premium" }`

If the authorization relay reports that the client is ineligible, the relay
returns `402 entitlementRequired`.

## Commands

```bash
cargo fmt
cargo test
cargo run
```

## Architecture Rules

This project follows `NETPLAY_ENGINEERING_RULES.md`.

Key constraints:

- SOLID principles.
- Separation of concern.
- No god files.
- 400-500 non-comment lines maximum per Rust source file.
- Module front matter on every Rust module.
- `///` docs for public APIs.
- Tests added with each behavior slice.

## MVP Protocol Direction

The server is authoritative for:

- room creation
- invite-code lookup
- player slot assignment
- player status
- input ownership validation
- coarse frame validation

Desktop clients remain authoritative for:

- running emulator cores
- creating save-state snapshots
- loading snapshots
- sampling controller input
- rendering gameplay
- storing local netplay autosaves

See `docs/protocol-mvp.md` for the first protocol outline.
