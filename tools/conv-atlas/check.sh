#!/usr/bin/env bash
# conv-atlas gate: python lint+tests, app typecheck+tests+build. Kept out of the
# cargo gate on purpose (o-prime ruling 2026-10-12). Runs niced (fleet rule C12).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
echo "== export (python)"
(cd "$here/export" && nice -n 19 uv run ruff check src tests && nice -n 19 uv run ruff format --check src tests && nice -n 19 uv run pytest -q)
echo "== app (typescript)"
(cd "$here/app" && pnpm -s install --frozen-lockfile && pnpm -s fixture >/dev/null && nice -n 19 pnpm -s typecheck && nice -n 19 pnpm -s test && nice -n 19 pnpm -s build >/dev/null)
echo "conv-atlas check: green"
