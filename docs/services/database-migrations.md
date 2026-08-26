# Database migrations
**Built**: 2026-08-26 (worker pij-recent-cicada, w-migrations) · **Code**: `crates/store/src/lib.rs` (`MIGRATOR`, `migrate`, `connect_lazy`), `crates/store/migrations/*.sql`, call-site `serve()` in `crates/daemon/src/main.rs` · **Tests**: `crates/store/tests/pg_migrations.rs`, `crates/daemon/tests/boot_contract.rs`
**Updated**: 2026-08-26 — the boot contract gained an automated test and the logged URL is now redacted (config packet, `a99ceed`).

Forward-only schema evolution for the Postgres+pgvector store. Plain numbered `.sql` files under `crates/store/migrations/`, embedded into the binary at compile time by `sqlx::migrate!`, applied once at daemon boot. No down migrations, no versioning tool, no wiping the database to change it. Task-oriented guide: [`docs/how/database.md`](../how/database.md).

## Key decisions
- **Boot migrates, and boot fails loud.** The daemon is the single writer, so startup is the only moment migrations can run unraced. Failure exits nonzero naming the database and `docker compose up -d`. A writer that cannot reach its own schema has nothing useful to serve — one loud refusal beats a guaranteed error per request. PRD req 37.
- **The database is named, the password never is.** Both the boot error and the success log run the URL through `fs3_core::redact_url_password` first. A message that helps only if it identifies *which* database was tried will otherwise print the credentials sitting in that same URL.
- **Lazy pool survives, at runtime.** `connect_lazy` stays: wiring never blocks on a connection and `GET /health` keeps answering through a database *outage*, so `flowspace3 ping` still separates "daemon down" from "database down". Boot strict, runtime forgiving — the split is deliberate and is why `docs/how/architecture.md:103-110` was rewritten rather than contradicted.
- **`connect_lazy` shares `connect`'s acquire timeout.** The two constructors must not disagree about how long "unreachable" takes. See gotchas — this was a real 30s hole.
- **Embedded, not deployed.** `sqlx::migrate!("./migrations")` bakes the history into the binary: nothing to ship alongside, and no way for the binary and the files to disagree about what the schema is.
- **Throwaway database per test.** `pg_migrations.rs` creates `fs3_migrations_<clock^pid^counter>`, migrates it, drops it. Same isolation reasoning as `pg_round_trip`'s `unique_blob`, and stricter — these tests DROP what they name, against a shared 5433 stack.
- **Assert against the embedded set, never a literal.** The bootstrap test compares applied versions to `MIGRATOR.iter()`, not `[1]`. Hardcoding today's version writes a test that goes quietly stale the day `0002` lands.
- **`0001_elements.sql`, not `0001_init.sql`.** In a forward-only numbered series the description says what the file *did*; the next one is `0002_edges`. "init" carries no information past the first migration.

## Gotchas learned
- **`connect_lazy` had no `acquire_timeout` while `connect` did.** First use of an absent store waited sqlx's 30-second default before erroring — the exact silence `CONNECT_TIMEOUT` exists to refuse (`lib.rs:27-32`), and boot migration *is* such a first use. Measured 30.0s → 5.6s after the fix. Nothing in the suite asserts how *long* a failure takes, so this was only visible with a stopwatch on the real binary.
- **`main.rs` is thin ground — keep it that way deliberately.** Every other daemon test builds `Config`/`AppState`/`Router` by hand, so a behaviour change in `main` can ship with a fully green suite. The boot contract went unguarded for exactly that reason until `boot_contract.rs` pinned it. Anything you add to boot needs its own real-binary test or it is not covered.
- **An error that names a URL names its password.** The first cut of the boot failure printed `state.config.database.url` verbatim — helpful, and a credential leak into stderr and into the INFO line on every *successful* boot. `boot_contract.rs:72` now asserts the password never appears. Any new message that identifies the database must go through `redact_url_password`.
- **"Same versions applied" is not idempotence.** A test asserting the version set would pass even if every migration re-ran. `installed_on` compared before/after is the assertion that actually bites.
- **`installed_on` is `TIMESTAMPTZ` and sqlx has no date/time feature enabled here.** Decoding it fails at runtime; cast with `installed_on::text` in the query.
- **`CREATE DATABASE` takes no bind parameters** and cannot run in a transaction. The test builds the identifier from hex, so there is nothing to quote out of. `DROP DATABASE ... WITH (FORCE)` needs PG13+ (the image is pg16).
- **`Drop` cannot await**, so throwaway-database cleanup is an explicit call. A test that panics first leaves one empty `fs3_migrations_*` database behind — visible, harmless, and a truthful record that the run failed.
- **Never edit an applied migration.** sqlx checksums each one; changing a file that has already run makes the daemon refuse to start. The fix for a mistake is another numbered file.

## Verify
```bash
docker compose up -d                                  # pgvector/pgvector:pg16 on 127.0.0.1:5433
cargo test -p fs3-store --test pg_migrations          # fresh bootstrap + repeat-is-a-no-op
cargo test -p fs3-daemon --test boot_contract         # unreachable store → nonzero, names the fix, hides the password
harness checks                                        # full gate incl. arch (sqlx stays in fs3-store)
```
`boot_contract.rs` is the automated form of what used to be a manual smoke. The happy path still is one — no test asserts the success log — so after touching boot:
```bash
cargo run -p fs3-daemon        # INFO "store schema is current" → "fs3 daemon listening"
```
Leak check after a test run:
```bash
docker exec flowspace3-db psql -U flowspace3 -tAc \
  "SELECT datname FROM pg_database WHERE datname LIKE 'fs3_migrations_%'"   # expect empty
```
As of `a99ceed`: `cargo test -p fs3-daemon --test boot_contract` green (5.8s — the fail-fast budget is the point, `PATIENCE` is 60s and a hang is a failure).

## Adding a migration
Add `crates/store/migrations/000N_<what_it_does>.sql`, restart the daemon. That is the whole procedure. To undo `0002`, write `0003`.
