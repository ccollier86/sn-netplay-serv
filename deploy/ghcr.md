# GHCR Image Publishing

Coolify should deploy the netplay relay from GHCR:

```text
ghcr.io/ccollier86/sb-netplay-serv:latest
```

Build locally:

```bash
./scripts/build-ghcr-image.sh
```

The script derives the complete current Git commit SHA, embeds it in the relay
binary and OCI metadata, and tags the same image with both that immutable SHA
and `latest`. It refuses a tag that does not equal the current commit.

The build script defaults to `linux/amd64` so Mac builds still produce the
Coolify server image. Override `SB_NETPLAY_IMAGE_PLATFORM` only if the deploy
host changes architecture.

Push locally:

```bash
export GHCR_TOKEN=<github-token-with-write-packages>
./scripts/push-ghcr-image.sh
```

If you have already run `docker login ghcr.io` with a token that has
`write:packages`, the push script can use that existing Docker login without
`GHCR_TOKEN`.

Use the immutable SHA tag for a pinned deployment:

```bash
relay_sha="$(git rev-parse HEAD)"
./scripts/build-ghcr-image.sh "$relay_sha"
./scripts/push-ghcr-image.sh "$relay_sha"
```

Then set this in Coolify:

```text
SB_NETPLAY_IMAGE=ghcr.io/ccollier86/sb-netplay-serv:<complete-commit-sha>
```

`GET /health` exposes `buildSha`, `imageIdentity`, `version`, and the supported
protocol range. Verify those values against the immutable tag after deployment;
do not infer the running revision from `latest` alone.
