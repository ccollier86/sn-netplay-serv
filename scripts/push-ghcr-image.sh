#!/usr/bin/env bash
set -euo pipefail

image_repository="${SB_NETPLAY_IMAGE_REPOSITORY:-ghcr.io/ccollier86/sb-netplay-serv}"
tag="${1:-latest}"
ghcr_user="${GHCR_USER:-ccollier86}"

if [[ -n "${GHCR_TOKEN:-}" ]]; then
  printf '%s' "${GHCR_TOKEN}" | docker login ghcr.io -u "${ghcr_user}" --password-stdin
else
  echo "GHCR_TOKEN not set; using existing Docker credentials for ghcr.io."
fi

docker push "${image_repository}:${tag}"

if [[ "${tag}" != "latest" ]]; then
  docker push "${image_repository}:latest"
fi

echo "Pushed ${image_repository}:${tag}"
