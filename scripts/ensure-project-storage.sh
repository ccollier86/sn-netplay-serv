#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
# shellcheck source=project-storage-env.sh
# shellcheck disable=SC1091
source "$ROOT/scripts/project-storage-env.sh"

ensure_directory() {
  local path="$1"
  case "$path" in
    "$PROJECT_CACHE_DIR"|"$PROJECT_CACHE_DIR"/*|\
    "$PROJECT_BUILD_DIR"|"$PROJECT_BUILD_DIR"/*|\
    "$PROJECT_ARTIFACT_DIR"|"$PROJECT_ARTIFACT_DIR"/*|\
    "$PROJECT_SCRATCH_DIR"|"$PROJECT_LOG_DIR") ;;
    *)
      echo "sb-netplay-serv storage: refusing undeclared directory: $path" >&2
      exit 1
      ;;
  esac
  if [[ -L "$path" ]]; then
    echo "sb-netplay-serv storage: refusing symlinked directory: $path" >&2
    exit 1
  fi
  if [[ -e "$path" && ! -d "$path" ]]; then
    echo "sb-netplay-serv storage: expected a directory: $path" >&2
    exit 1
  fi
  /bin/mkdir -p "$path"
}

for parent in \
  /Volumes/code-bank/caches \
  /Volumes/code-bank/build \
  /Volumes/code-bank/artifacts \
  /Volumes/code-bank/tmp/scratch \
  /Volumes/code-bank/logs; do
  if [[ ! -d "$parent" || -L "$parent" ]]; then
    echo "sb-netplay-serv storage: unsafe canonical parent: $parent" >&2
    exit 1
  fi
done

for path in \
  "$PROJECT_CACHE_DIR" \
  "$PROJECT_BUILD_DIR" \
  "$CARGO_TARGET_DIR" \
  "$PROJECT_BUILD_DIR/node" \
  "$PROJECT_BUILD_DIR/gradle" \
  "$PROJECT_BUILD_DIR/package-staging" \
  "$PROJECT_ARTIFACT_DIR" \
  "$PROJECT_ARTIFACT_DEV_DIR" \
  "$PROJECT_ARTIFACT_NIGHTLY_DIR" \
  "$PROJECT_RELEASE_DIR" \
  "$PROJECT_DIAGNOSTICS_DIR" \
  "$PROJECT_SCRATCH_DIR" \
  "$PROJECT_LOG_DIR"; do
  ensure_directory "$path"
done

project-storage-check "$ROOT"
