#!/usr/bin/env bash
# Release preflight — replicate every release-job command LOCALLY before any
# tag cycle. Jordan, 2026-08-26: 8 tag cycles in one day, at least 5 of them
# locally catchable. No tag cycle without this green (see docs/services/
# ci-release.md → Release runbook).
#
# Legs map 1:1 onto .github/workflows/release.yml job names:
#   macos-builds (aarch64-apple-darwin)   -> legs A/B/C1/C2 (this mac IS the
#                                            macos-14 target architecture)
#   container-builds (x86_64-unknown-linux-gnu) -> leg D
#   container-builds (aarch64-unknown-linux-gnu) -> leg E (opt-in: PREFLIGHT_ARM=1)
#
# C2 is the runner simulation: docker masked out of PATH and the database
# pointed at a dead port, which is what a macOS runner actually looks like.
# It never touches the shared compose stack — masking, not stopping.
set -uo pipefail

cd "$(dirname "$0")/../.."
RESULTS=()
FAILED=0

leg() { # leg <name> <job-label> <command...>
  local name="$1" job="$2"; shift 2
  printf '\n=== %s (%s) ===\n' "$name" "$job"
  if "$@"; then
    RESULTS+=("PASS  $name  -> $job")
  else
    RESULTS+=("FAIL  $name  -> $job")
    FAILED=1
  fi
}

TARGET_MAC="aarch64-apple-darwin"
BIN="target/$TARGET_MAC/release/flowspace3"

# --- A: the mac build command, verbatim -------------------------------------
leg "A build --locked" "macos-builds ($TARGET_MAC)" \
  cargo build --locked --release --target "$TARGET_MAC" -p fs3-cli

# --- B: the smoke block, verbatim -------------------------------------------
smoke() {
  "$BIN" --version && "$BIN" daemon --help >/dev/null && "$BIN" doctor --help >/dev/null
}
leg "B smoke-run" "macos-builds ($TARGET_MAC)" smoke

# --- C1: the mac fast tier, verbatim, normal environment --------------------
leg "C1 fast tier (normal env)" "macos-builds ($TARGET_MAC)" \
  cargo test --workspace --lib --exclude fs3-store

# --- C2: the mac fast tier under a RUNNER SIMULATION ------------------------
# macOS runners have no docker binary, no engine, no Postgres. Mask docker out
# of PATH (shim dir carries only the tools a runner has) and point the database
# at a dead port. The shared stack is never stopped.
runner_sim() {
  local shim; shim="$(mktemp -d)"
  for tool in cargo rustc rustup git cc clang make ld pkg-config; do
    p="$(command -v "$tool" 2>/dev/null)" && ln -sf "$p" "$shim/$tool"
  done
  command -v docker >/dev/null && echo "  (masking $(command -v docker))"
  env -i \
    HOME="$HOME" \
    PATH="$shim:/usr/bin:/bin:/usr/sbin:/sbin" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" \
    RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    FS3_DATABASE__URL="postgres://flowspace3:flowspace3@127.0.0.1:9/flowspace3" \
    FS3_TEST_DATABASE_URL="postgres://flowspace3:flowspace3@127.0.0.1:9/flowspace3" \
    cargo test --workspace --lib --exclude fs3-store
  local rc=$?
  rm -rf "$shim"
  return $rc
}
leg "C2 fast tier (runner sim: no docker, no db)" "macos-builds ($TARGET_MAC)" runner_sim

# --- D: the linux x86_64 leg via the plan-002 build container ---------------
linux_leg() {
  local target="$1" platform="linux/arm64"
  case "$target" in x86_64-*) platform="linux/amd64" ;; esac
  FS3_TARGET="$target" ./docker/scripts/build.sh || return 1
  # CI runs this smoke natively on an x86_64 runner; locally the platform has
  # to be named or OrbStack runs the binary under the wrong loader.
  docker run --rm --platform "$platform" -v fs3-bin:/bins:ro debian:trixie-slim \
    "/bins/$target/release/flowspace3" --version
}

leg "D linux x86_64 container build + smoke" "container-builds (x86_64-unknown-linux-gnu)" \
  linux_leg x86_64-unknown-linux-gnu

if [ "${PREFLIGHT_ARM:-0}" = "1" ]; then
  leg "E linux aarch64 container build" "container-builds (aarch64-unknown-linux-gnu)" \
    linux_leg aarch64-unknown-linux-gnu
fi

printf '\n===== RELEASE PREFLIGHT =====\n'
printf '%s\n' "${RESULTS[@]}"
if [ "$FAILED" = 1 ]; then
  printf '\nRED — do NOT cycle the tag. Fix the failing leg first.\n'
  exit 1
fi
printf '\nGREEN — safe to cycle the tag.\n'
