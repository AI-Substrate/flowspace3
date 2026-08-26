#!/usr/bin/env bash
# Stack verbs over the ROOT docker-compose.yml (db-only by ruling
# 2026-08-26-daemon-native-on-host — the daemon runs natively on the host and
# never becomes a compose service). Usage: stack.sh up|down|status|logs|exec [args]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
ENGINE="${FS3_ENGINE:-docker}"

compose() { "$ENGINE" compose -f "$ROOT/docker-compose.yml" "$@"; }

case "${1:-}" in
  up)     shift; compose up -d "$@" ;;
  down)   shift; compose down "$@" ;;          # NEVER -v from the paved surface
  status) shift; compose ps "$@" ;;
  logs)   shift; compose logs "$@" ;;
  exec)   shift; compose exec "$@" ;;
  *)
    echo "usage: stack.sh up|down|status|logs|exec [args]" >&2
    exit 2 ;;
esac
