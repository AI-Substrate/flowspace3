#!/usr/bin/env bash
# Build a workspace package for a target triple, entirely inside the pinned
# build container. See docs/how/docker.md.
#
#   FS3_TARGET   target triple (default aarch64-unknown-linux-gnu);
#                darwin targets are refused — those build natively on the host
#   FS3_PACKAGE  cargo -p package (default fs3-cli)
#   FS3_BIN_NAME produced binary name (default flowspace3 — req 51 ships ONE
#                binary; package fs3-cli produces the `flowspace3` bin whose
#                daemon is the `flowspace3 daemon` subcommand)
#   FS3_ENGINE   engine binary (default docker); DRY_RUN=1 echoes only
#
# Caches live in named volumes with explicit fixed names:
#   fs3-cargo-registry -> $CARGO_HOME/registry (shared by all targets)
#   fs3-cargo-target   -> CARGO_TARGET_DIR (cargo separates per triple under
#                         target/<triple>/, so each target warms its own cache)
#   fs3-bin            -> published binaries, one subdir per triple
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
ENGINE="${FS3_ENGINE:-docker}"
DRY_RUN="${DRY_RUN:-0}"
PACKAGE="${FS3_PACKAGE:-fs3-cli}"
BIN_NAME="${FS3_BIN_NAME:-flowspace3}"
TARGET="${FS3_TARGET:-aarch64-unknown-linux-gnu}"

case "$TARGET" in
  *-apple-*)
    echo "refusing: $TARGET builds NATIVELY on the mac host (Apple SDK licensing)" >&2
    exit 2 ;;
  x86_64-unknown-linux-*) PLATFORM="linux/amd64" ;;
  *)                      PLATFORM="linux/arm64" ;;
esac

IMAGE="fs3-build:latest"
RUSTUP_VOL="fs3-rustup-arm64"
if [ "$PLATFORM" = "linux/amd64" ]; then
  IMAGE="fs3-build-amd64:latest"
  RUSTUP_VOL="fs3-rustup-x64"
fi

REGISTRY_VOL="fs3-cargo-registry"
TARGET_VOL="fs3-cargo-target"
BIN_VOL="fs3-bin"
# Toolchain caches are per-architecture: one shared rustup home would mix
# aarch64 and x86_64 toolchain binaries and break whichever arch mounts second.
run() { echo "+ $*"; if [ "$DRY_RUN" != "1" ]; then "$@"; fi; }


if ! "$ENGINE" image inspect "$IMAGE" >/dev/null 2>&1; then
  run "$ENGINE" build --platform "$PLATFORM" -f "$ROOT/docker/Dockerfile.build" -t "$IMAGE" "$ROOT"
fi

for vol in "$REGISTRY_VOL" "$TARGET_VOL" "$BIN_VOL" "$RUSTUP_VOL"; do
  "$ENGINE" volume inspect "$vol" >/dev/null 2>&1 || run "$ENGINE" volume create "$vol"
done

# musl targets: full static (Debian's musl-gcc defaults to PIE-dynamic);
# linux-gnu: ort-sys bundles prebuilt onnxruntime C++ objects and the plain
# cc link driver never adds libstdc++ on its own.
CARGO_STATIC=""
case "$TARGET" in
  *-musl)      CARGO_STATIC="-C target-feature=+crt-static -C relocation-model=static" ;;
  *-linux-gnu) CARGO_STATIC="-C linker=g++" ;;
esac

# The repo pins channel=stable (rust-toolchain.toml), so the ACTIVE toolchain
# is whatever stable the persisted rustup home holds — ensure its std for the
# requested target (no-op once cached in the fs3-rustup volume).
start=$SECONDS
run "$ENGINE" run --rm --platform "$PLATFORM" \
  -v "$ROOT:/src:ro" \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  -e RUSTFLAGS="$CARGO_STATIC" \
  -v "$REGISTRY_VOL:/usr/local/cargo/registry" \
  -v "$TARGET_VOL:/target" \
  -v "$RUSTUP_VOL:/usr/local/rustup" \
  "$IMAGE" \
  bash -c 'rustup target add "$1" >/dev/null 2>&1 || true; exec cargo build --locked --release --target "$1" -p "$2"' _ "$TARGET" "$PACKAGE"
build_s=$((SECONDS - start))

# Publish into the output volume (staged + atomic mv: a live-mounted
# executable never hits Text-file-busy).
OUT="$TARGET/release/$BIN_NAME"
case "$TARGET" in *windows*) OUT="$TARGET/release/$BIN_NAME.exe" ;; esac
run "$ENGINE" run --rm --platform "$PLATFORM" \
  -v "$TARGET_VOL:/target:ro" \
  -v "$BIN_VOL:/out" \
  "$IMAGE" \
  sh -c "mkdir -p '/out/${TARGET}/release' && cp '/target/$OUT' '/out/.staging' && mv -f '/out/.staging' '/out/$OUT'"

echo "cargo build [$TARGET] $PACKAGE: ${build_s}s (platform=$PLATFORM, FS3_ENGINE=$ENGINE)"
