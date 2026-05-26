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

preflight_failures=()

record_preflight_failure() {
  local target="$1"
  local message="$2"
  preflight_failures+=("${target}: ${message}")
}

check_command() {
  local target="$1"
  local command="$2"
  local label="$3"
  if ! command -v "$command" >/dev/null 2>&1; then
    record_preflight_failure "$target" "$label is required before initializing mobile packaging"
  fi
}

check_existing_dir_env() {
  local target="$1"
  local name="$2"
  local label="$3"
  local value="${!name:-}"
  if [[ -z "$value" || ! -d "$value" ]]; then
    record_preflight_failure "$target" "$name must point at $label before initializing mobile packaging"
  fi
}

check_existing_dir_env_any() {
  local target="$1"
  local label="$2"
  shift 2
  local name
  local value
  for name in "$@"; do
    value="${!name:-}"
    if [[ -n "$value" && -d "$value" ]]; then
      return
    fi
  done
  record_preflight_failure "$target" "$* must point at $label before initializing mobile packaging"
}

check_rust_target() {
  local target="$1"
  local rust_target="$2"
  if ! command -v rustup >/dev/null 2>&1; then
    return
  fi
  if ! rustup target list --installed | grep -qx "$rust_target"; then
    record_preflight_failure "$target" "Rust mobile target is missing: $rust_target"
  fi
}

preflight_platform() {
  local target="$1"

  check_command "$target" rustup "rustup"
  case "$target" in
    android)
      check_existing_dir_env "$target" ANDROID_HOME "the Android SDK"
      check_existing_dir_env_any "$target" "the Android NDK" NDK_HOME ANDROID_NDK_HOME
      check_command "$target" java "Java"
      check_rust_target "$target" aarch64-linux-android
      check_rust_target "$target" armv7-linux-androideabi
      check_rust_target "$target" i686-linux-android
      check_rust_target "$target" x86_64-linux-android
      ;;
    ios)
      check_command "$target" xcodebuild "Xcode"
      check_command "$target" pod "CocoaPods"
      check_rust_target "$target" aarch64-apple-ios
      check_rust_target "$target" aarch64-apple-ios-sim
      check_rust_target "$target" x86_64-apple-ios
      ;;
    *)
      fail "unsupported platform $target"
      ;;
  esac
}

preflight_selected_platforms() {
  local target

  preflight_failures=()
  for target in "$@"; do
    echo "preflighting ${target} mobile packaging prerequisites"
    preflight_platform "$target"
  done

  if ((${#preflight_failures[@]} > 0)); then
    echo "mobile packaging init failed: missing mobile packaging prerequisites:" >&2
    printf -- '- %s\n' "${preflight_failures[@]}" >&2
    exit 1
  fi
}

init_platform() {
  local target="$1"

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

selected_platforms=()
case "$platform" in
  android)
    selected_platforms=(android)
    ;;
  ios)
    selected_platforms=(ios)
    ;;
  all)
    selected_platforms=(android ios)
    ;;
esac

preflight_selected_platforms "${selected_platforms[@]}"
for target in "${selected_platforms[@]}"; do
  init_platform "$target"
done
