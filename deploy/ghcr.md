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
binary and OCI metadata, and tags the image as `latest`. Publishing is done
locally with the Docker CLI; repository pushes do not build or publish images.

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

Keep this set in Coolify:

```text
SB_NETPLAY_IMAGE=ghcr.io/ccollier86/sb-netplay-serv:latest
```

`GET /health` exposes `buildSha`, `imageIdentity`, `version`, and the supported
protocol range. After redeploying `latest`, verify that `buildSha` equals the
commit embedded by the local build before beginning live client acceptance.
