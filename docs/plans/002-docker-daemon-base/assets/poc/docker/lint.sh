#!/usr/bin/env bash
# tk-0105: engine-agnostic proof.
#  1. every script honours FS3_ENGINE (default docker)
#  2. compose.yaml validates against the compose spec (`compose config`)
#  3. lint: no Docker-exclusive features in the compose/scripts surface
# Exits non-zero on any violation.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="${FS3_ENGINE:-docker}"
fail=0

echo "== FS3_ENGINE honoured in every script =="
for f in "$HERE"/build.sh "$HERE"/reload.sh "$HERE"/down.sh "$HERE"/lint.sh; do
  if grep -q 'FS3_ENGINE' "$f"; then
    echo "ok   $(basename "$f")"
  else
    echo "FAIL $(basename "$f"): no FS3_ENGINE"; fail=1
  fi
done

echo "== compose spec validation =="
if "$ENGINE" compose --project-name fs3-poc -f "$HERE/compose.yaml" config -q; then
  echo "ok   compose config validates"
else
  echo "FAIL compose config rejected the file"; fail=1
fi

echo "== lint: docker-exclusive features =="
# Forbidden tokens (compose-spec violations / docker-only):
#   develop/watch  -> compose watch (docker-only, not in compose-spec runtimes)
#   docker.sock    -> host socket mount (engine-specific)
#   gpus/device    -> docker-specific device passthrough syntax
PATTERN='develop:|watch:|docker\.sock|gpus:'
hits="$(grep -nE "$PATTERN" "$HERE/compose.yaml" || true)"
if [ -z "$hits" ]; then
  echo "ok   no docker-exclusive features in compose.yaml"
else
  echo "FAIL docker-exclusive features found:"; echo "$hits"; fail=1
fi

exit $fail
