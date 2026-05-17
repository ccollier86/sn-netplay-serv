#!/usr/bin/env bash
set -euo pipefail

image_repository="${SB_NETPLAY_IMAGE_REPOSITORY:-ghcr.io/ccollier86/sb-netplay-serv}"
tag="${1:-latest}"

docker build \
  --pull \
  -t "${image_repository}:${tag}" \
  -t "${image_repository}:latest" \
  .

echo "Built ${image_repository}:${tag}"
