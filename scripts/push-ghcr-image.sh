#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
image_repository="${SB_NETPLAY_IMAGE_REPOSITORY:-ghcr.io/ccollier86/sb-netplay-serv}"
build_sha="$(git -C "$ROOT" rev-parse HEAD)"
ghcr_user="${GHCR_USER:-ccollier86}"

if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
  echo "Commit all relay changes before pushing the latest image." >&2
  exit 2
fi

image_revision="$(docker image inspect "${image_repository}:latest" --format '{{ index .Config.Labels "org.opencontainers.image.revision" }}')"
if [[ "$image_revision" != "$build_sha" ]]; then
  echo "Refusing to push image revision ${image_revision}; expected ${build_sha}." >&2
  exit 2
fi

if [[ -n "${GHCR_TOKEN:-}" ]]; then
  printf '%s' "${GHCR_TOKEN}" | docker login ghcr.io -u "${ghcr_user}" --password-stdin
else
  echo "GHCR_TOKEN not set; using existing Docker credentials for ghcr.io."
fi

docker push "${image_repository}:latest"

echo "Pushed ${image_repository}:latest (${build_sha})"
