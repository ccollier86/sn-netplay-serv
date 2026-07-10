#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
# shellcheck source=project-storage-env.sh
# shellcheck disable=SC1091
source "$ROOT/scripts/project-storage-env.sh"

if [[ "$#" -eq 0 ]]; then
  echo 'usage: scripts/with-project-env.sh <command> [args ...]' >&2
  exit 2
fi

cd "$ROOT"
exec "$@"
