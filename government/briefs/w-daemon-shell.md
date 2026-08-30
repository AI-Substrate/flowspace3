# Worker brief — daemon shell prototype: host-native watcher + web service · (seat at canary)
**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-26 · one bounded task · PROTOTYPE (inspiration, not shipped code)

## Context

Jordan's ruling (`.harness/government/rulings/2026-08-26-daemon-native-on-host.md`): the fs3 daemon runs NATIVELY on the host — file watching and runtime-addable folders don't work from inside containers. This prototype is the learning vehicle: "a daemon that runs outside of Docker with a little web service and a little file watcher … our daemon sort of shell that we take inspiration from, but maybe don't directly use." **Everything must be cross-platform: mac, linux, AND windows.**

## The job

A standalone Rust prototype at `pocs/daemon-shell/` — its OWN cargo project, **NOT a workspace member** (add empty `[workspace]` to its Cargo.toml; the arch check asserts exactly 7 workspace crates and must stay green; give it a .gitignore for target/).

1. **Web service**: axum, local-only (`127.0.0.1`, configurable port): `GET /health` · `GET /status` (uptime, watched roots, per-root event/dirty counts) · `POST /watch {path}` + `DELETE /watch {path}` — runtime add/remove of watched roots (the `flowspace3 add path` shape) · `GET /dirty` (current debounced dirty set).
2. **File watcher**: the `notify` crate (FSEvents / inotify / ReadDirectoryChangesW backends), recursive per root, **10s debounce** (fs3's default) coalescing into an in-memory dirty set (path → last-event time, drained via /dirty). Ignore noise dirs (.git, target, node_modules) — hardcoded list is fine in a prototype.
3. **Cross-platform discipline**: std/portable paths only, no unix-isms (no signal handling beyond ctrl-c, no unix sockets); if a platform needs different behaviour, isolate it and document it. It must COMPILE for all three OSes: prove `cargo build` for the windows + linux targets if toolchains permit (coordinate NOTHING with ox — just note if cross toolchains are absent and let the mac build + code review stand for portability).
4. **Prove on mac NOW**: run it; add a scratch dir; touch/edit/delete files incl. a burst (100 files) and an ignored-dir write; show /dirty reflects reality after debounce; measure event→dirty latency. Record actual transcripts.
5. **LEARNINGS.md** (the real deliverable): what the real daemon must know — notify backend quirks per OS (FSEvents coalescing, rename semantics, recursive-watch costs), debounce design that survived contact, watched-root lifecycle (overlaps, nesting, removal races), what you'd do differently. Plus README.md (run instructions).

## Rules & fence

- Fence: `pocs/daemon-shell/**` ONLY. Scratch `.harness/temp/w-daemon-shell/**`. Everything else excluded — do not touch workspace crates, docs elsewhere, government, .claude.
- Prototype grade: no ports/DI ceremony needed, BUT keep the scanner rule in spirit — watcher core (debounce/dirty-set logic) as pure functions with unit tests; the axum+notify shell around it. That split is itself a learning for the real daemon.
- `harness checks` must STAY green (your project is outside the workspace so mostly no-op — verify you didn't break the 7-crate assertion or docs links).
- Commit + push per coherent unit: scoped adds of `pocs/daemon-shell/**` only, push-first protocol (ruling 2026-08-26-commit-push-as-you-go.md).
- Report to pij-instant-lynx: claim · run transcript evidence · LEARNINGS highlights · files. Deviations = stop-and-ask.

Ack by pij message, then go.
