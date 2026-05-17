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
SB_NETPLAY_IMAGE=ghcr.io/ccollier86/sb-netplay-serv:latest
SB_NETPLAY_BIND_ADDR=0.0.0.0:8077
SB_NETPLAY_DESKTOP_AUTHORIZE_URL=https://assets.shadowboy.app/internal/desktop/netplay/authorize
SB_NETPLAY_LICENSE_INTERNAL_SECRET=<server-to-server-secret>
SB_NETPLAY_ADMIN_TOKEN=<long-random-operator-token>
```

Recommended:

```text
RUST_LOG=sb_netplay_serv=info,tower_http=info
SB_NETPLAY_LOG_FORMAT=json
SB_NETPLAY_TRUST_PROXY_HEADERS=true
SB_NETPLAY_RATE_CREATE_ROOM_PER_MINUTE=12
SB_NETPLAY_RATE_WS_JOIN_PER_MINUTE=30
SB_NETPLAY_RATE_ROOM_STATUS_PER_MINUTE=120
```

Store these values in Coolify's environment/secret UI, not in git.

Use `coolify.env.local` as the paste source on this machine. It is ignored by
git. `coolify.env.example` is the tracked template.

## Remote Debugging

Public health check:

```bash
curl -fsS https://netplay.shadowboy.app/health
```

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
whether slots are occupied. It does not expose desktop tokens, install secrets,
ROM names, or gameplay payloads.

Coolify container logs should show structured JSON when
`SB_NETPLAY_LOG_FORMAT=json` is set. Use those logs for request failures,
license authority outages, and rate-limit rejections.

## Scaling Note

Run one instance for now. A second instance would need sticky WebSocket routing
or a shared room backend because rooms are intentionally process-local in the
MVP.
