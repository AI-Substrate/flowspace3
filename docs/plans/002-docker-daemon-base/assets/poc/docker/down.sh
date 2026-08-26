#!/usr/bin/env bash
# Paved down: stops the POC stack. NEVER deletes volumes — plain `down`
# without -v; the build/cache/output volumes are external and survive even
# a stack deletion by construction.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="${FS3_ENGINE:-docker}"
DRY_RUN="${DRY_RUN:-0}"
run() { echo "+ $*"; if [ "$DRY_RUN" != "1" ]; then "$@"; fi; }

run "$ENGINE" compose --project-name fs3-poc -f "$HERE/compose.yaml" down
