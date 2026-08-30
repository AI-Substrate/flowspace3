# Worker brief — sqlx embedded migrations + database how-to · pij-recent-cicada
**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-26 · one bounded task, no review fleet

## The job

Wire **forward-only sqlx embedded migrations** into `fs3-store`, running at daemon startup against our docker PG, and write a dead-simple how-to. Jordan's bar (verbatim): "we didnt have to blow away db every time to alter things, but … i dont want complex and annoying versioning, so basic simple is what i want - elegant."

1. **Migrations dir**: `crates/store/migrations/` with `0001_init.sql` — fold the store's EXISTING schema creation (find how `elements` etc. are created today — likely inline SQL in fs3-store or its tests) into this first migration, including `CREATE EXTENSION IF NOT EXISTS vector`. Plain SQL files, numbered, forward-only. No down migrations.
2. **Store API**: expose `pub async fn migrate(pool: &PgPool) -> …` in fs3-store using `sqlx::migrate!` (embeds the files in the binary). Replace any ad-hoc schema setup with it.
3. **Daemon boot**: call it once at startup in fs3-daemon (after config, before serving) — the daemon is the single writer, so startup is the only migration point. Failure = actionable error naming the db url and the fix, daemon exits nonzero.
4. **Tests** (against the compose PG on 127.0.0.1:5433, `FS3_TEST_DATABASE_URL` override per existing store tests): fresh-db bootstrap works; applying twice is a no-op (idempotent); existing `pg_round_trip` tests still green using the migrated schema.
5. **How-to**: `docs/how/database.md` — KEEP IT SHORT (a page). Three sections: (a) "Changing the schema" = add one numbered .sql file, restart the daemon, done — never wipe the db; (b) "How it works" = sqlx tracking table, forward-only, runs at daemon boot; (c) "Using the database from a service" = the injection story: `PgPool` lives in `AppState` (built once in the composition root from config), handlers/workers clone it (pools are cheap Arc clones), store functions take `&PgPool` — a new service needing db capability adds nothing but a parameter; NO new port (workshop 001: the store is concrete, Postgres is a requirement not a variable).

## Rules

- Architecture binds: sqlx stays inside fs3-store (the arch check enforces it — daemon gets migrate() through the store's API, never sqlx directly); no new ports; no mocking crates.
- Use the running compose db (`docker compose up -d` if down; port 5433). Never touch ox's POC stack or its volumes.
- Fence: `crates/store/**`, the single startup call-site in `crates/daemon/src/`, `docs/how/database.md`. Scratch `.harness/temp/w-migrations/**`. Everything else excluded — especially `.harness/government/**`, `.claude/**`, `docs/plans/**`, other crates.
- **No commits** — working tree only; s001 is landing on main, o-prime coordinates.
- Gates before reporting: `harness checks` + `cargo test --workspace` green (docker up).
- Report to pij-instant-lynx: claim · files · gate output · how-to path · observations. Deviations = stop-and-ask.

Ack by pij message, then go.
