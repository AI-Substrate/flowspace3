#!/usr/bin/env bash
# tk-0101: build the POC /health daemon entirely inside the build container,
# from the mac host, targeting aarch64 linux (the engine's native arch).
#
# Engine-agnostic: honours FS3_ENGINE (default docker). DRY_RUN=1 echoes
# commands without executing (podman-compat proof, dw-0107).
#
# Source mounts READ-ONLY; all cargo writes go to two NAMED cache volumes
# with explicit fixed names (also declared in compose.yaml, external) so
# `engine run -v` and compose always hit the SAME volumes regardless of
# compose project-name prefixing:
#   fs3-poc-cargo-registry  -> $CARGO_HOME/registry
#   fs3-poc-cargo-target    -> CARGO_TARGET_DIR
# The fresh binary lands in a third named output volume:
#   fs3-poc-bin             -> copied out of the target volume post-build
#
# Never rebuilds any image for a source change: the image is built once
# here; source changes reuse the cached layers + warm volumes.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="${FS3_ENGINE:-docker}"
DRY_RUN="${DRY_RUN:-0}"
IMAGE="fs3-poc-build:latest"
CRATE="$HERE/daemon"

REGISTRY_VOL="fs3-poc-cargo-registry"
TARGET_VOL="fs3-poc-cargo-target"
BIN_VOL="fs3-poc-bin"

run() {
  echo "+ $*"
  if [ "$DRY_RUN" != "1" ]; then "$@"; fi
}

# Build the pinned image once (cached afterwards; not part of the dev loop).
if ! "$ENGINE" image inspect "$IMAGE" >/dev/null 2>&1; then
  run "$ENGINE" build -f "$HERE/Dockerfile.build" -t "$IMAGE" "$HERE"
fi

# Idempotently ensure the named volumes exist.
for vol in "$REGISTRY_VOL" "$TARGET_VOL" "$BIN_VOL"; do
  "$ENGINE" volume inspect "$vol" >/dev/null 2>&1 || run "$ENGINE" volume create "$vol"
done

start=$SECONDS
run "$ENGINE" run --rm \
  -v "$CRATE:/src:ro" \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  -v "$REGISTRY_VOL:/usr/local/cargo/registry" \
  -v "$TARGET_VOL:/target" \
  "$IMAGE" \
  cargo build --locked --release
build_s=$((SECONDS - start))

# Publish the binary into the shared output volume the daemon service mounts.
run "$ENGINE" run --rm \
  -v "$TARGET_VOL:/target:ro" \
  -v "$BIN_VOL:/out" \
  "$IMAGE" \
  sh -c 'cp /target/release/fs3-poc-daemon /out/.staging && mv -f /out/.staging /out/fs3-poc-daemon'

echo "cargo build: ${build_s}s (FS3_ENGINE=$ENGINE)"
