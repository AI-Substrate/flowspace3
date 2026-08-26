# Ruling — daemon runs natively on the host; compose is PG-only
**By**: Jordan ("Agreed. I think we switched from running the daemon inside Docker and leave it out on the host.") · **Recorded**: 2026-08-26 · pij-instant-lynx

- The fs3 daemon is a native host process on mac/linux/windows. Drivers: file watching (FSEvents/inotify/RDCW don't cross VM boundaries reliably) and runtime-addable folders (`flowspace3 add path` can't retro-mount into a running container).
- Docker keeps: Postgres/pgvector (data), the build container (cross-platform + CI). Containerized daemon = possible later server-mode profile only.
- PRD req 33 amended in place (docs/plans/prd/base-prd.md). s002 phase 2 re-scopes at gate-open: compose stays db-only; docker/ scripts + harness extension + cross-platform matrix remain in scope; "daemon service in compose" drops.
- **Everything cross-platform** (Jordan, same ruling): mac, linux, AND windows for all host-side components.
- Prototype ordered: a standalone daemon shell (little web service + file watcher, notify + axum, host-native) as inspiration/learning — not necessarily shipped code.
