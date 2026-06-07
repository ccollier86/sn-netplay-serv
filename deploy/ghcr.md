# GHCR Image Publishing

Coolify should deploy the netplay relay from GHCR:

```text
ghcr.io/ccollier86/sb-netplay-serv:latest
```

Build locally:

```bash
./scripts/build-ghcr-image.sh latest
```

The build script defaults to `linux/amd64` so Mac builds still produce the
Coolify server image. Override `SB_NETPLAY_IMAGE_PLATFORM` only if the deploy
host changes architecture.

Push locally:

```bash
export GHCR_TOKEN=<github-token-with-write-packages>
./scripts/push-ghcr-image.sh latest
```

If you have already run `docker login ghcr.io` with a token that has
`write:packages`, the push script can use that existing Docker login without
`GHCR_TOKEN`.

Use a version tag when you want a pinned deployment:

```bash
./scripts/build-ghcr-image.sh 0.1.0
./scripts/push-ghcr-image.sh 0.1.0
```

Then set this in Coolify:

```text
SB_NETPLAY_IMAGE=ghcr.io/ccollier86/sb-netplay-serv:0.1.0
```

The `latest` tag is convenient while we are iterating. A version tag is safer
once the desktop client depends on a specific relay contract.
