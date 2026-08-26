#!/usr/bin/env bash
# One-shot command inside the pinned build container, joined to the compose
# network so tests can reach the db service. Default: the paved workspace test.
#
#   docker/scripts/run.sh                    -> cargo test --workspace
#   docker/scripts/run.sh cargo clippy ...   -> any command in-container
#
# Exports FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@db:5432/flowspace3
# (the shipped default targets 127.0.0.1:5433 — that is the CONTAINER itself
# in-network, so store's pg_round_trip would panic without this override).
#
# Requires the stack up (docker/scripts/stack.sh up). Honours FS3_ENGINE/DRY_RUN.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
ENGINE="${FS3_ENGINE:-docker}"
DRY_RUN="${DRY_RUN:-0}"
TARGET="${FS3_TARGET:-aarch64-unknown-linux-gnu}"
TEST_DB_URL="${FS3_TEST_DATABASE_URL:-postgres://flowspace3:flowspace3@db:5432/flowspace3}"

case "$TARGET" in
  *-apple-*)
    echo "refusing: $TARGET builds NATIVELY on the mac host (Apple SDK licensing)" >&2
    exit 2 ;;
  x86_64-unknown-linux-*) PLATFORM="linux/amd64" ;;
  *)                      PLATFORM="linux/arm64" ;;
esac

# musl targets: full static (Debian's musl-gcc defaults to PIE-dynamic);
# linux-gnu: ort-sys bundles prebuilt onnxruntime C++ objects; drive the link
# with g++ so libstdc++ is appended automatically.
RUSTFLAGS_X=""
case "$TARGET" in
  *-musl)      RUSTFLAGS_X="-C target-feature=+crt-static -C relocation-model=static" ;;
  *-linux-gnu) RUSTFLAGS_X="-C linker=g++" ;;
esac

IMAGE="fs3-build:latest"
if [ "$PLATFORM" = "linux/amd64" ]; then IMAGE="fs3-build-amd64:latest"; fi

run() { echo "+ $*"; if [ "$DRY_RUN" != "1" ]; then "$@"; fi; }

if ! "$ENGINE" image inspect "$IMAGE" >/dev/null 2>&1; then
  run "$ENGINE" build --platform "$PLATFORM" -f "$ROOT/docker/Dockerfile.build" -t "$IMAGE" "$ROOT"
fi

for vol in fs3-cargo-registry fs3-cargo-target fs3-rustup; do
  "$ENGINE" volume inspect "$vol" >/dev/null 2>&1 || run "$ENGINE" volume create "$vol"
done

# Compose network name follows the root compose file's project (dir name).
NETWORK="$("$ENGINE" compose -f "$ROOT/docker-compose.yml" config --format json 2>/dev/null | sed -n 's/.*"name":"\([^"]*\)".*/\1/p' | head -1)"
NETWORK="${NETWORK:-flowspace3_default}"

CMD=("$@")
if [ ${#CMD[@]} -eq 0 ]; then CMD=(cargo test --workspace); fi
# Best-effort: give the container the host engine's socket (read-only) so
# tests that shell out to an engine (fs3 doctor) work in-container too.
SOCKET_ARGS=()
if [ -n "${DOCKER_HOST:-}" ] && [[ "$DOCKER_HOST" == unix://* ]]; then
  SOCKET_ARGS=(-v "${DOCKER_HOST#unix://}:/var/run/docker.sock:ro")
else
  for cand in "$HOME/.orbstack/run/docker.sock" /var/run/docker.sock "${XDG_RUNTIME_DIR:-/run/user/$UID}/podman/podman.sock"; do
    if [ -S "$cand" ]; then SOCKET_ARGS=(-v "$cand:/var/run/docker.sock:ro"); break; fi
  done
fi

# Forward the daemon's shipped default database address (127.0.0.1:5433) to
# the compose db service INSIDE the run container. This keeps the shipped
# defaults true in-container AND keeps negative tests (unreachable-store
# boot contract) negative — no env overrides that would mask bad configs.
exec_run=(
  "$ENGINE" run --rm --platform "$PLATFORM"
  --network "$NETWORK"
  "${SOCKET_ARGS[@]}"
  -v "$ROOT:/src:ro"
  -w /src
  -e CARGO_TARGET_DIR=/target
  -e RUSTFLAGS="$RUSTFLAGS_X"
  -e FS3_TEST_DATABASE_URL="$TEST_DB_URL"
  -v fs3-cargo-registry:/usr/local/cargo/registry
  -v fs3-cargo-target:/target
  -v fs3-rustup:/usr/local/rustup
  "$IMAGE"
  bash -c 'socat TCP-LISTEN:5433,bind=127.0.0.1,fork,reuseaddr TCP:db:5432 >/dev/null 2>&1 & exec "$@"' _ "${CMD[@]}"
)

if [ "$DRY_RUN" = "1" ]; then
  echo "+ ${exec_run[*]} ${CMD[*]}"
else
  exec "${exec_run[@]}"
fi
