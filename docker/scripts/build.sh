#!/usr/bin/env bash
# Build fs3-daemon (or any -p package) for a target triple, entirely inside
# the pinned build container. See docs/how/docker.md.
#
#   FS3_TARGET  target triple (default aarch64-unknown-linux-gnu);
#               darwin targets are refused — those build natively on the host
#   FS3_PACKAGE cargo -p package (default fs3-daemon)
#   FS3_ENGINE  engine binary (default docker); DRY_RUN=1 echoes only
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
TARGET="${FS3_TARGET:-aarch64-unknown-linux-gnu}"
PACKAGE="${FS3_PACKAGE:-fs3-daemon}"
BIN_NAME="$PACKAGE"

case "$TARGET" in
  *-apple-*)
    echo "refusing: $TARGET builds NATIVELY on the mac host (Apple SDK licensing)" >&2
    exit 2 ;;
  x86_64-unknown-linux-*) PLATFORM="linux/amd64" ;;
  *)                      PLATFORM="linux/arm64" ;;
esac

IMAGE="fs3-build:latest"
if [ "$PLATFORM" = "linux/amd64" ]; then IMAGE="fs3-build-amd64:latest"; fi

REGISTRY_VOL="fs3-cargo-registry"
TARGET_VOL="fs3-cargo-target"
BIN_VOL="fs3-bin"

run() { echo "+ $*"; if [ "$DRY_RUN" != "1" ]; then "$@"; fi; }

if ! "$ENGINE" image inspect "$IMAGE" >/dev/null 2>&1; then
  run "$ENGINE" build --platform "$PLATFORM" -f "$ROOT/docker/Dockerfile.build" -t "$IMAGE" "$ROOT"
fi

for vol in "$REGISTRY_VOL" "$TARGET_VOL" "$BIN_VOL" fs3-rustup; do
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

start=$SECONDS
run "$ENGINE" run --rm --platform "$PLATFORM" \
  -v "$ROOT:/src:ro" \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  -e RUSTFLAGS="$CARGO_STATIC" \
  -v "$REGISTRY_VOL:/usr/local/cargo/registry" \
  -v "$TARGET_VOL:/target" \
  -v fs3-rustup:/usr/local/rustup \
  "$IMAGE" \
  cargo build --locked --release --target "$TARGET" -p "$PACKAGE"
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
