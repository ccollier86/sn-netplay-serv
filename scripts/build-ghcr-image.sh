#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
# Docker/OrbStack owns image-layer build state on the external Docker backend.
# The project helper still validates that this command is running from the
# canonical external checkout with external package/build cache variables.
# shellcheck source=project-storage-env.sh
# shellcheck disable=SC1091
source "$ROOT/scripts/project-storage-env.sh"

image_repository="${SB_NETPLAY_IMAGE_REPOSITORY:-ghcr.io/ccollier86/sb-netplay-serv}"
tag="${1:-latest}"
platform="${SB_NETPLAY_IMAGE_PLATFORM:-linux/amd64}"

cd "$ROOT"
docker build \
  --pull \
  --platform "${platform}" \
  -t "${image_repository}:${tag}" \
  -t "${image_repository}:latest" \
  .

echo "Built ${image_repository}:${tag} (${platform})"
