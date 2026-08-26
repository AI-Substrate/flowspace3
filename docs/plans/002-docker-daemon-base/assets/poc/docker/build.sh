#!/usr/bin/env bash
# tk-0101 + cross-platform matrix: build the POC /health daemon entirely
# inside the build container, from the mac host.
#
# Target selection: FS3_TARGET (default aarch64-unknown-linux-gnu).
#   aarch64/x86_64-unknown-linux-{gnu,musl} -> built in the container whose
#     arch matches the target (x86_64 targets run --platform linux/amd64)
#   x86_64-pc-windows-gnu                   -> cross-built in the arm64
#     container via mingw-w64; produce-only (never executed on mac/linux)
#
# Engine-agnostic: honours FS3_ENGINE (default docker). DRY_RUN=1 echoes
# commands without executing (podman-compat proof, dw-0107).
#
# Source mounts READ-ONLY; all cargo writes go to two NAMED cache volumes
# with explicit fixed names (also declared in compose.yaml, external) so
# `engine run -v` and compose always hit the SAME volumes regardless of
# compose project-name prefixing:
#   fs3-poc-cargo-registry  -> $CARGO_HOME/registry   (shared by all targets)
#   fs3-poc-cargo-target    -> CARGO_TARGET_DIR       (cargo separates caches
#                              per triple under target/<triple>/, so every
#                              target gets its own warm cache in one volume)
# Fresh binaries land in a third named output volume, one subdir per target.
#
# Never rebuilds any image for a source change: the image is built once
# here; source changes reuse the cached layers + warm volumes.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="${FS3_ENGINE:-docker}"
DRY_RUN="${DRY_RUN:-0}"
TARGET="${FS3_TARGET:-aarch64-unknown-linux-gnu}"
IMAGE="fs3-poc-build:latest"
CRATE="$HERE/daemon"

# x86_64 linux targets build inside an amd64 instance of the SAME image
# (OrbStack: Rosetta; podman: qemu) — no cross toolchain, identical recipe.
case "$TARGET" in
  x86_64-unknown-linux-*) PLATFORM="linux/amd64" ;;
  *)                      PLATFORM="linux/arm64" ;;
esac

# Output path in the shared bin volume, per-target subdirectory.
BIN_NAME="fs3-poc-daemon"
case "$TARGET" in *windows*) OUT="$TARGET/release/$BIN_NAME.exe" ;; *) OUT="$TARGET/release/$BIN_NAME" ;; esac

REGISTRY_VOL="fs3-poc-cargo-registry"
TARGET_VOL="fs3-poc-cargo-target"
BIN_VOL="fs3-poc-bin"

run() {
  echo "+ $*"
  if [ "$DRY_RUN" != "1" ]; then "$@"; fi
}

# Build the pinned image once per platform (cached afterwards; not part of
# the dev loop). Image name carries the platform suffix for amd64.
if [ "$PLATFORM" = "linux/amd64" ]; then IMAGE="fs3-poc-build-amd64:latest"; fi
if ! "$ENGINE" image inspect "$IMAGE" >/dev/null 2>&1; then
  run "$ENGINE" build --platform "$PLATFORM" -f "$HERE/Dockerfile.build" -t "$IMAGE" "$HERE"
fi

# Idempotently ensure the named volumes exist.
for vol in "$REGISTRY_VOL" "$TARGET_VOL" "$BIN_VOL"; do
  "$ENGINE" volume inspect "$vol" >/dev/null 2>&1 || run "$ENGINE" volume create "$vol"
done

# musl targets: force full static linking (Debian's musl-gcc defaults to
# PIE-dynamic, which defeats musl's purpose); gnu/windows need nothing.
CARGO_STATIC=""
case "$TARGET" in *-musl) CARGO_STATIC="-C target-feature=+crt-static -C relocation-model=static" ;; esac

start=$SECONDS
run "$ENGINE" run --rm --platform "$PLATFORM" \
  -e CARGO_TARGET_DIR=/target \
  -e RUSTFLAGS="$CARGO_STATIC" \
  -v "$CRATE:/src:ro" \
  -w /src \
  -v "$REGISTRY_VOL:/usr/local/cargo/registry" \
  -v "$TARGET_VOL:/target" \
  "$IMAGE" \
  cargo build --locked --release --target "$TARGET"
build_s=$((SECONDS - start))

# Publish the binary into the shared output volume (staged + atomic mv so a
# live-mounted executable never hits Text-file-busy).
run "$ENGINE" run --rm --platform "$PLATFORM" \
  -v "$TARGET_VOL:/target:ro" \
  -v "$BIN_VOL:/out" \
  "$IMAGE" \
  sh -c "mkdir -p '/out/${TARGET}/release' && cp '/target/$OUT' '/out/.staging' && mv -f '/out/.staging' '/out/$OUT'"

echo "cargo build [$TARGET]: ${build_s}s (platform=$PLATFORM, FS3_ENGINE=$ENGINE)"
