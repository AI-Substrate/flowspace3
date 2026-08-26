#!/usr/bin/env bash
# Engine-agnostic lint for the docker surface: every script honours
# FS3_ENGINE, the root compose file validates against the compose spec, and
# no Docker-exclusive feature is used anywhere.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="${FS3_ENGINE:-docker}"
fail=0

echo "== FS3_ENGINE honoured in every script =="
for f in "$HERE"/build.sh "$HERE"/stack.sh "$HERE"/lint.sh; do
  if grep -q 'FS3_ENGINE' "$f"; then echo "ok   $(basename "$f")"; else
    echo "FAIL $(basename "$f"): no FS3_ENGINE"; fail=1; fi
done
for f in "$HERE"/run.sh; do
  if grep -q 'FS3_ENGINE\|"${ENGINE}"' "$f"; then echo "ok   $(basename "$f")"; else
    echo "FAIL $(basename "$f"): no engine indirection"; fail=1; fi
done

echo "== compose spec validation =="
if "$ENGINE" compose -f "$(dirname "$HERE")/../docker-compose.yml" config -q; then
  echo "ok   compose config validates"
else
  echo "FAIL compose config rejected docker-compose.yml"; fail=1
fi

echo "== lint: docker-exclusive features =="
hits="$(grep -nE 'develop:|watch:|docker\.sock|gpus:' "$(dirname "$HERE")/../docker-compose.yml" || true)"
if [ -z "$hits" ]; then echo "ok   no docker-exclusive features"; else
  echo "FAIL docker-exclusive features found:"; echo "$hits"; fail=1; fi

exit $fail
