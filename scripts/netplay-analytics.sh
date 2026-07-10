#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

if [[ -f ".env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source ".env"
  set +a
fi

# Secrets load first; the storage helper then rejects any conflicting path
# override and ensures Cargo cannot fall back to the legacy repository target.
# shellcheck source=project-storage-env.sh
# shellcheck disable=SC1091
source "$ROOT/scripts/project-storage-env.sh"

cargo run --bin netplay_analytics -- "$@"
