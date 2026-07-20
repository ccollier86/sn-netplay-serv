# Coolify Deployment

The netplay relay is stateless. For the first production slice, do not add a
database or object storage. Rooms, input relay state, metrics counters, and
invite codes live in memory and disappear when the container restarts.

## Application

- Type: Docker Compose
- Compose file: `docker-compose.coolify.yml`
- Published port: `8077`
- Health check path: `/health`
- Public domain example: `https://netplay.shadowboy.app`
- WebSockets: enabled by Coolify's reverse proxy
- TLS: managed by Coolify for the public domain

The compose file deploys `SB_NETPLAY_IMAGE`, which should point at GHCR. See
`deploy/ghcr.md` for build/push commands.

## Environment

Required:

```text
SERVICE_FQDN_NETPLAY=netplay.shadowboy.app
SERVICE_URL_NETPLAY=https://netplay.shadowboy.app
SERVICE_URL_NETPLAY_8077=https://netplay.shadowboy.app
SB_NETPLAY_BIND_ADDR=0.0.0.0:8077
SB_NETPLAY_AUTHORIZE_URL=https://assets.shadowboy.app/internal/netplay/authorize
SB_NETPLAY_DESKTOP_AUTHORIZE_URL=
SB_NETPLAY_LICENSE_INTERNAL_SECRET=<server-to-server-secret>
SB_NETPLAY_ADMIN_TOKEN=<long-random-operator-token>
SB_NETPLAY_V5_ENABLED=true
SB_NETPLAY_MIN_PROTOCOL_ANDROID=4
SB_NETPLAY_MIN_PROTOCOL_IOS=4
SB_NETPLAY_MIN_PROTOCOL_DESKTOP=4
```

Recommended:

```text
RUST_LOG=sb_netplay_serv=info,tower_http=info
SB_NETPLAY_LOG_FORMAT=json
SB_NETPLAY_TRUST_PROXY_HEADERS=true
SB_NETPLAY_RATE_CREATE_ROOM_PER_MINUTE=12
SB_NETPLAY_RATE_WS_JOIN_PER_MINUTE=30
SB_NETPLAY_RATE_ROOM_STATUS_PER_MINUTE=120
SB_NETPLAY_RECONNECT_GRACE_SECONDS=90
SB_NETPLAY_RUNNER_HANDOFF_GRACE_SECONDS=60
SB_NETPLAY_HEARTBEAT_STALE_SECONDS=15
SB_NETPLAY_HEARTBEAT_DISCONNECT_SECONDS=30
SB_NETPLAY_ROOM_IDLE_SECONDS=300
SB_NETPLAY_LOBBY_IDLE_SECONDS=3600
SB_NETPLAY_POSTGRES_URL=postgres://postgres:<password>@metrics.shadowboy.app:5433/postgres?sslmode=require
SB_NETPLAY_POSTGRES_EVENTS_TABLE=netplay_room_events
SB_NETPLAY_POSTGRES_PERFORMANCE_TABLE=netplay_performance_samples
SB_NETPLAY_TELEMETRY_QUEUE_CAPACITY=20000
SB_NETPLAY_TELEMETRY_BATCH_SIZE=250
SB_NETPLAY_TELEMETRY_FLUSH_MS=1000
SB_NETPLAY_VOICE_BROKER_URL=https://voice.shadowboy.app
SB_NETPLAY_VOICE_BROKER_TOKEN=<same value as SB_WEBRTC_RELAY_TOKEN>
SB_NETPLAY_VOICE_BROKER_TIMEOUT_MS=2500
```

Store these values in Coolify's environment/secret UI, not in git.

The protocol switches above deliberately deploy dual v4/v5 support while each
platform minimum remains 4. A new v5-only client can create and join v5 rooms,
and an already-published v4 client remains confined to v4 rooms. Raise one
platform minimum to 5 only after its forced-update policy has made the old
client obsolete. Never use these settings to reinterpret an existing room or
raise every platform minimum before the corresponding clients ship.

`SB_NETPLAY_POSTGRES_URL` is optional for gameplay but recommended for long-term
diagnostics. When present, the server checks and installs the analytics schema
at startup and logs whether Postgres telemetry is ready.

Use `coolify.env.local` as the paste source on this machine. It is ignored by
git. `coolify.env.example` is the tracked template.

## Remote Debugging

Public health check:

```bash
curl -fsS https://netplay.shadowboy.app/health
```

The response must identify the immutable deployed image and source revision:

```json
{
  "status": "ok",
  "buildSha": "<complete-commit-sha>",
  "imageIdentity": "ghcr.io/ccollier86/sb-netplay-serv:<complete-commit-sha>",
  "version": "0.1.0",
  "minSupportedProtocolVersion": 4,
  "maxSupportedProtocolVersion": 5
}
```

Compare `buildSha` with the requested `SB_NETPLAY_IMAGE` before live client
acceptance. A healthy response from a different SHA is not a successful
rollout.

Authenticated metrics:

```bash
curl -fsS \
  -H "Authorization: Bearer $SB_NETPLAY_ADMIN_TOKEN" \
  https://netplay.shadowboy.app/internal/metrics
```

Authenticated active-room snapshot:

```bash
curl -fsS \
  -H "Authorization: Bearer $SB_NETPLAY_ADMIN_TOKEN" \
  https://netplay.shadowboy.app/internal/rooms
```

The room snapshot includes invite codes, room statuses, player slots, and
whether slots are occupied. It does not expose client tokens, install secrets,
ROM names, or gameplay payloads.

Authenticated room event history:

```bash
curl -fsS \
  -H "Authorization: Bearer $SB_NETPLAY_ADMIN_TOKEN" \
  "https://netplay.shadowboy.app/internal/rooms/AB23-CD/events?limit=100"
```

Authenticated recent event history across rooms:

```bash
curl -fsS \
  -H "Authorization: Bearer $SB_NETPLAY_ADMIN_TOKEN" \
  "https://netplay.shadowboy.app/internal/recent-events?limit=100"
```

Coolify container logs should show structured JSON when
`SB_NETPLAY_LOG_FORMAT=json` is set. Use those logs for request failures,
license authority outages, and rate-limit rejections.

## Scaling Note

Run one instance for now. A second instance would need sticky WebSocket routing
or a shared room backend because rooms are intentionally process-local in the
MVP.
