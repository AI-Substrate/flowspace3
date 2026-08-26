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

# --- B2: the binary must agree with the version being RELEASED --------------
# The defect this exists for (req-0060): release-please's `simple` strategy
# bumps .release-please-manifest.json and NOTHING in the Rust manifests, so
# v0.2.0 was tagged and published carrying a binary whose `--version` said
# 0.1.0. Nothing caught it — the smoke leg above RUNS `--version` and never
# reads what it prints.
#
# It is not cosmetic. The auto-updater compares its own compiled-in version
# against the newest published tag, so a binary that under-reports is
# permanently "older" than every release: it would re-download and re-swap once
# per check interval forever, raising a restart message that restarting cannot
# clear.
#
# The ORACLE is .release-please-manifest.json, not Cargo.toml. That is the
# whole point: a binary is always built from Cargo.toml, so comparing the two
# compares a thing with itself and can never fail. The manifest is what the
# next tag will be named after, and the bug was precisely that it and Cargo.toml
# had drifted apart. Keeping them equal is the job of the
# `x-release-please-version` annotation on `[workspace.package] version`.
version_truthful() {
  local releasing binary
  releasing=$(jq -r '."."' .release-please-manifest.json)
  binary=$("$BIN" --version | awk '{print $NF}')

  if [ "$releasing" != "$binary" ]; then
    printf 'version mismatch: the next release is %s, the built binary says %s\n' \
      "$releasing" "$binary" >&2
    printf 'Cargo.toml [workspace.package] version has drifted from\n' >&2
    printf '.release-please-manifest.json — check the x-release-please-version\n' >&2
    printf 'annotation in Cargo.toml and the extra-files entry in\n' >&2
    printf 'release-please-config.json.\n' >&2
    return 1
  fi
  printf 'the binary agrees with the version being released: %s\n' "$binary"
}
leg "B2 version truth (binary == released version)" "macos-builds ($TARGET_MAC)" version_truthful

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
