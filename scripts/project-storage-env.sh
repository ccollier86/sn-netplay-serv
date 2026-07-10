#!/usr/bin/env bash

# Canonical, side-effect-free storage environment for sb-netplay-serv.
# Source this file before Cargo, Bun, Gradle, Docker, test, or diagnostic work.

_sb_netplay_storage_error() {
  echo "sb-netplay-serv storage: $*" >&2
}

_sb_netplay_storage_set_exact() {
  local name="$1"
  local expected="$2"
  local current="${!name-}"

  if [[ -n "$current" && "$current" != "$expected" ]]; then
    _sb_netplay_storage_error "$name conflicts with the canonical path: $current"
    return 1
  fi
  export "$name=$expected"
}

_sb_netplay_storage_device_id() {
  stat -f '%d' "$1" 2>/dev/null || stat -c '%d' "$1" 2>/dev/null
}

_sb_netplay_storage_load() {
  local expected_root='/Volumes/code-bank/code/sb-desktop/sb-netplay-serv'
  local expected_drive='/Volumes/code-bank'
  local helper_root
  local source_root
  local root_device
  local drive_device
  local source_device

  helper_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." 2>/dev/null && pwd -P)" || return 1
  source_root="$(cd "$expected_root" 2>/dev/null && pwd -P)" || {
    _sb_netplay_storage_error "canonical source checkout is unavailable: $expected_root"
    return 1
  }
  if [[ "$helper_root" != "$expected_root" || "$source_root" != "$expected_root" ]]; then
    _sb_netplay_storage_error "helper is not running from the canonical physical checkout: $helper_root"
    return 1
  fi
  if [[ ! -d "$expected_drive" || ! -w "$expected_drive" ]]; then
    _sb_netplay_storage_error "development drive is unavailable or not writable: $expected_drive"
    return 1
  fi

  root_device="$(_sb_netplay_storage_device_id /)"
  drive_device="$(_sb_netplay_storage_device_id "$expected_drive")"
  source_device="$(_sb_netplay_storage_device_id "$expected_root")"
  if [[ -z "$root_device" || -z "$drive_device" || -z "$source_device" ]]; then
    _sb_netplay_storage_error 'could not verify filesystem device ownership'
    return 1
  fi
  if [[ "$root_device" == "$drive_device" || "$drive_device" != "$source_device" ]]; then
    _sb_netplay_storage_error 'source and storage roots are not on the expected external filesystem'
    return 1
  fi

  _sb_netplay_storage_set_exact PROJECT_NAME 'sb-netplay-serv' || return 1
  _sb_netplay_storage_set_exact DEV_DRIVE "$expected_drive" || return 1
  _sb_netplay_storage_set_exact PROJECT_SOURCE_ROOT "$expected_root" || return 1
  _sb_netplay_storage_set_exact PROJECT_CACHE_DIR '/Volumes/code-bank/caches/sb-netplay-serv' || return 1
  _sb_netplay_storage_set_exact PROJECT_BUILD_DIR '/Volumes/code-bank/build/sb-netplay-serv' || return 1
  _sb_netplay_storage_set_exact PROJECT_ARTIFACT_DIR '/Volumes/code-bank/artifacts/sb-netplay-serv' || return 1
  _sb_netplay_storage_set_exact PROJECT_ARTIFACT_DEV_DIR '/Volumes/code-bank/artifacts/sb-netplay-serv/dev' || return 1
  _sb_netplay_storage_set_exact PROJECT_ARTIFACT_NIGHTLY_DIR '/Volumes/code-bank/artifacts/sb-netplay-serv/nightly' || return 1
  _sb_netplay_storage_set_exact PROJECT_RELEASE_DIR '/Volumes/code-bank/artifacts/sb-netplay-serv/release' || return 1
  _sb_netplay_storage_set_exact PROJECT_DIAGNOSTICS_DIR '/Volumes/code-bank/artifacts/sb-netplay-serv/diagnostics' || return 1
  _sb_netplay_storage_set_exact PROJECT_SCRATCH_DIR '/Volumes/code-bank/tmp/scratch/sb-netplay-serv' || return 1
  _sb_netplay_storage_set_exact PROJECT_LOG_DIR '/Volumes/code-bank/logs/sb-netplay-serv' || return 1

  _sb_netplay_storage_set_exact CARGO_HOME '/Volumes/code-bank/caches/cargo/home' || return 1
  _sb_netplay_storage_set_exact RUSTUP_HOME '/Volumes/code-bank/caches/rustup/home' || return 1
  _sb_netplay_storage_set_exact CARGO_TARGET_DIR '/Volumes/code-bank/build/sb-netplay-serv/rust-target' || return 1
  _sb_netplay_storage_set_exact SCCACHE_DIR '/Volumes/code-bank/caches/sccache' || return 1
  _sb_netplay_storage_set_exact CCACHE_DIR '/Volumes/code-bank/caches/ccache' || return 1
  _sb_netplay_storage_set_exact NPM_CONFIG_CACHE '/Volumes/code-bank/caches/npm' || return 1
  _sb_netplay_storage_set_exact npm_config_cache '/Volumes/code-bank/caches/npm' || return 1
  _sb_netplay_storage_set_exact BUN_INSTALL_CACHE_DIR '/Volumes/code-bank/caches/bun/install-cache' || return 1
  _sb_netplay_storage_set_exact BUN_CACHE_DIR '/Volumes/code-bank/caches/bun/install-cache' || return 1
  _sb_netplay_storage_set_exact GRADLE_USER_HOME '/Volumes/code-bank/caches/gradle/home' || return 1

  export PROJECT_STORAGE_ENV_LOADED=1
}

if _sb_netplay_storage_load; then
  _sb_netplay_storage_status=0
else
  _sb_netplay_storage_status=$?
fi

unset -f _sb_netplay_storage_error _sb_netplay_storage_set_exact
unset -f _sb_netplay_storage_device_id _sb_netplay_storage_load

if [[ "$_sb_netplay_storage_status" -ne 0 ]]; then
  _sb_netplay_storage_failed_status="$_sb_netplay_storage_status"
  unset _sb_netplay_storage_status
  # exit is the direct-execution fallback.
  # shellcheck disable=SC2317
  return "$_sb_netplay_storage_failed_status" 2>/dev/null || exit "$_sb_netplay_storage_failed_status"
fi
unset _sb_netplay_storage_status
