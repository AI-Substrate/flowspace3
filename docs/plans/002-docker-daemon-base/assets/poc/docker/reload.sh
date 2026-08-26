#!/usr/bin/env bash
# tk-0104: db-safe reload loop. Rebuild in the build container (warm caches,
# zero image rebuilds) then recreate ONLY the daemon service. The db service
# is never stopped, restarted, or recreated.
#
# Prints each container's StartedAt after every reload; dw-0106 proof is two
# consecutive runs showing db StartedAt unchanged while daemon's advances.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="${FS3_ENGINE:-docker}"
DRY_RUN="${DRY_RUN:-0}"
run() { echo "+ $*"; if [ "$DRY_RUN" != "1" ]; then "$@"; fi; }

"$HERE/build.sh"

run "$ENGINE" compose --project-name fs3-poc -f "$HERE/compose.yaml" \
  up -d --no-deps --force-recreate daemon

for c in fs3-poc-db fs3-poc-daemon; do
  printf '%s StartedAt: %s\n' "$c" "$("$ENGINE" inspect -f '{{.State.StartedAt}}' "$c")"
done
