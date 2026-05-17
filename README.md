# ShadowBoy Netplay Server

Rust relay server for ShadowBoy Desktop netplay. The server coordinates invite
codes, license verification, player slots, compatibility checks, state sync, and
frame-numbered input relay. It never runs emulator cores, never receives ROMs,
and never streams gameplay video.

## Current Slice

Implemented:

- `GET /health`
- `POST /v1/rooms`
- `GET /v1/rooms/{invite_code}/status`
- `GET /v1/ws?inviteCode=AB23-CD&role=host`
- `GET /v1/ws?inviteCode=AB23-CD&role=guest`
- HTTP-backed license-authority client
- server-side netplay entitlement authorization through the metadata service
- fakeable `LicenseAuthority` trait
- in-memory room registry
- room WebSocket upgrade path
- host socket attachment to Player 1
- guest socket join as Player 2
- room state broadcasts over WebSocket
- basic WebSocket `ping` / `pong`
- compatibility fingerprint handling
- ready/start handling
- host snapshot chunk and manifest relay
- frame-numbered input validation and relay
- abandoned waiting-room expiration
- invite-code parsing and generation
- Player 1 / Player 2 slot model
- compatibility fingerprint comparison
- input-frame ownership and frame-bound validation
- snapshot chunk and checksum validation helpers
- unit and route tests

Not implemented yet:

- ping/latency tracking
- resync protocol
- production deployment config

## Required Environment

```text
SB_NETPLAY_BIND_ADDR=127.0.0.1:8077
SB_NETPLAY_DESKTOP_AUTHORIZE_URL=https://assets.shadowboy.app/internal/desktop/netplay/authorize
SB_NETPLAY_LICENSE_INTERNAL_SECRET=<server-to-server-secret>
```

`SB_NETPLAY_BIND_ADDR` is optional and defaults to `127.0.0.1:8077`.
`SB_NETPLAY_LICENSE_VERIFY_URL` is still accepted as a legacy fallback for the
authorization URL.

## Desktop Authorization

Room creation requires the same protected-client identity Desktop already uses
for metadata, cheats, billing, and updates:

```text
Authorization: Bearer <desktop access token>
X-Install-Id: <installationId>
X-Req-Ts: <epoch milliseconds>
X-Req-Nonce: <unique nonce>
X-Req-Sig: <base64 signature>
```

The relay does not trust Electron's local premium state. It sends the access
token, install id, requested feature `netplay`, and the signed netplay request
proof to the metadata service using `SB_NETPLAY_LICENSE_INTERNAL_SECRET`.

The metadata service should validate:

- the desktop access token
- the install id
- the protected request signature when signature fields are present
- nonce/timestamp freshness
- app/version policy tied to the token
- premium or active-trial entitlement for `netplay`

Accepted response shapes:

- explicit authorization, for example `{ "authorized": true }`
- feature entitlement, for example `{ "features": { "netplay": true } }`
- premium/trial account state, for example `{ "accessStatus": "premium" }`

If authorization fails because the install is expired or not premium/trial, the
relay returns `402 entitlementRequired`.

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
