#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

platform="all"
preflight_only=false

for arg in "$@"; do
  case "$arg" in
    --platform=android|--platform=ios|--platform=all)
      platform="${arg#--platform=}"
      ;;
    --preflight-only)
      preflight_only=true
      ;;
    -h|--help)
      cat <<'EOF'
usage: scripts/init-mobile-packaging.sh [--platform=android|ios|all] [--preflight-only]

Initializes the generated Tauri mobile project(s), then runs strict packaging
verification for each selected platform.

With --preflight-only, checks platform SDK tools and Rust mobile targets without
running Tauri init or writing generated project files.
EOF
      exit 0
      ;;
    *)
      echo "mobile packaging init failed: unknown argument $arg" >&2
      exit 1
      ;;
  esac
done

fail() {
  echo "mobile packaging init failed: $1" >&2
  exit 1
}

require_command() {
  local command="$1"
  local label="$2"
  if ! command -v "$command" >/dev/null 2>&1; then
    fail "$label is required before initializing mobile packaging"
  fi
}

require_existing_dir_env() {
  local name="$1"
  local label="$2"
  local value="${!name:-}"
  if [[ -z "$value" || ! -d "$value" ]]; then
    fail "$name must point at $label before initializing mobile packaging"
  fi
}

require_rust_target() {
  local target="$1"
  if ! rustup target list --installed | grep -qx "$target"; then
    fail "Rust mobile target is missing: $target"
  fi
}

preflight_platform() {
  local target="$1"

  require_command rustup "rustup"
  case "$target" in
    android)
      require_existing_dir_env ANDROID_HOME "the Android SDK"
      require_existing_dir_env NDK_HOME "the Android NDK"
      require_command java "Java"
      require_rust_target aarch64-linux-android
      require_rust_target armv7-linux-androideabi
      require_rust_target i686-linux-android
      require_rust_target x86_64-linux-android
      ;;
    ios)
      require_command xcodebuild "Xcode"
      require_command pod "CocoaPods"
      require_rust_target aarch64-apple-ios
      require_rust_target aarch64-apple-ios-sim
      require_rust_target x86_64-apple-ios
      ;;
    *)
      fail "unsupported platform $target"
      ;;
  esac
}

init_platform() {
  local target="$1"

  echo "preflighting ${target} mobile packaging prerequisites"
  preflight_platform "$target"

  if [[ "$preflight_only" == true ]]; then
    echo "verifying ${target} mobile packaging preflight"
    scripts/verify-mobile-packaging.sh --strict --preflight-only "--platform=${target}"
    echo "${target} mobile packaging prerequisites are ready"
    return
  fi

  echo "initializing ${target} Tauri mobile project"
  npm --prefix crates/agent-tauri/frontend run tauri -- "$target" init --ci --skip-targets-install

  echo "verifying ${target} mobile packaging"
  scripts/verify-mobile-packaging.sh --strict "--platform=${target}"
}

case "$platform" in
  android)
    init_platform android
    ;;
  ios)
    init_platform ios
    ;;
  all)
    init_platform android
    init_platform ios
    ;;
esac
