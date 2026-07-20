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
build_sha="$(git -C "$ROOT" rev-parse HEAD)"
tag="${1:-$build_sha}"
platform="${SB_NETPLAY_IMAGE_PLATFORM:-linux/amd64}"

if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
  echo "Commit all relay changes before building an immutable image." >&2
  exit 2
fi

if [[ "$tag" != "$build_sha" ]]; then
  echo "Image tag must be the complete current commit SHA: $build_sha" >&2
  exit 2
fi

image_identity="${image_repository}:${tag}"

cd "$ROOT"
docker build \
  --pull \
  --platform "${platform}" \
  --build-arg "SB_NETPLAY_BUILD_SHA=${build_sha}" \
  --build-arg "SB_NETPLAY_IMAGE_IDENTITY=${image_identity}" \
  -t "${image_repository}:${tag}" \
  -t "${image_repository}:latest" \
  .

echo "Built ${image_identity} and ${image_repository}:latest (${platform}, ${build_sha})"
