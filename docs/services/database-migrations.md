# Database migrations
**Built**: 2026-08-26 (worker pij-recent-cicada, w-migrations) · **Code**: `crates/store/src/lib.rs` (`MIGRATOR`, `migrate`, `connect_lazy`), `crates/store/migrations/*.sql`, call-site `crates/daemon/src/main.rs` · **Tests**: `crates/store/tests/pg_migrations.rs`

Forward-only schema evolution for the Postgres+pgvector store. Plain numbered `.sql` files under `crates/store/migrations/`, embedded into the binary at compile time by `sqlx::migrate!`, applied once at daemon boot. No down migrations, no versioning tool, no wiping the database to change it. Task-oriented guide: [`docs/how/database.md`](../how/database.md).

## Key decisions
- **Boot migrates, and boot fails loud.** The daemon is the single writer, so startup is the only moment migrations can run unraced. Failure exits nonzero naming `database.url` and `docker compose up -d`. A writer that cannot reach its own schema has nothing useful to serve — one loud refusal beats a guaranteed error per request.
- **Lazy pool survives, at runtime.** `connect_lazy` stays: wiring never blocks on a connection and `GET /health` keeps answering through a database *outage*, so `flowspace3 ping` still separates "daemon down" from "database down". Boot strict, runtime forgiving — the split is deliberate and is why `docs/how/architecture.md:103-110` was rewritten rather than contradicted.
- **`connect_lazy` shares `connect`'s acquire timeout.** The two constructors must not disagree about how long "unreachable" takes. See gotchas — this was a real 30s hole.
- **Embedded, not deployed.** `sqlx::migrate!("./migrations")` bakes the history into the binary: nothing to ship alongside, and no way for the binary and the files to disagree about what the schema is.
- **Throwaway database per test.** `pg_migrations.rs` creates `fs3_migrations_<clock^pid^counter>`, migrates it, drops it. Same isolation reasoning as `pg_round_trip`'s `unique_blob`, and stricter — these tests DROP what they name, against a shared 5433 stack.
- **Assert against the embedded set, never a literal.** The bootstrap test compares applied versions to `MIGRATOR.iter()`, not `[1]`. Hardcoding today's version writes a test that goes quietly stale the day `0002` lands.
- **`0001_elements.sql`, not `0001_init.sql`.** In a forward-only numbered series the description says what the file *did*; the next one is `0002_edges`. "init" carries no information past the first migration.

## Gotchas learned
- **`connect_lazy` had no `acquire_timeout` while `connect` did.** First use of an absent store waited sqlx's 30-second default before erroring — the exact silence `CONNECT_TIMEOUT` exists to refuse (`lib.rs:27-32`), and boot migration *is* such a first use. Measured 30.0s → 5.6s after the fix. Nothing in the suite asserts how *long* a failure takes, so this was only visible with a stopwatch on the real binary.
- **`main.rs` is nearly untested ground.** Every daemon test builds `Config`/`AppState`/`Router` by hand, so a behaviour change in `main` can be shipped with a fully green suite. Only `tests/health.rs:118` (`the_real_binaries_agree_through_a_discovered_config`) runs the real binary — and it covers the *happy* boot-migration path incidentally, because its config takes the default 5433 URL. **The fail-fast contract has no automated test** (parked with o-prime for the config packet). Verify `main` changes by running the binary.
- **"Same versions applied" is not idempotence.** A test asserting the version set would pass even if every migration re-ran. `installed_on` compared before/after is the assertion that actually bites.
- **`installed_on` is `TIMESTAMPTZ` and sqlx has no date/time feature enabled here.** Decoding it fails at runtime; cast with `installed_on::text` in the query.
- **`CREATE DATABASE` takes no bind parameters** and cannot run in a transaction. The test builds the identifier from hex, so there is nothing to quote out of. `DROP DATABASE ... WITH (FORCE)` needs PG13+ (the image is pg16).
- **`Drop` cannot await**, so throwaway-database cleanup is an explicit call. A test that panics first leaves one empty `fs3_migrations_*` database behind — visible, harmless, and a truthful record that the run failed.
- **Never edit an applied migration.** sqlx checksums each one; changing a file that has already run makes the daemon refuse to start. The fix for a mistake is another numbered file.

## Verify
```bash
docker compose up -d                                  # pgvector/pgvector:pg16 on 127.0.0.1:5433
cargo test -p fs3-store --test pg_migrations          # fresh bootstrap + repeat-is-a-no-op
harness checks                                        # 5 gates incl. arch (sqlx stays in fs3-store)
```
Boot behaviour is not covered by the suite — smoke it directly:
```bash
# happy path: migrates, then serves
cargo run -p fs3-daemon        # INFO "store schema is current" → "fs3 daemon listening"

# failure path: config with database.url = postgres://nobody:nobody@127.0.0.1:1/nothing
FS3_CONFIG_DIR=<dir> ./target/debug/fs3-daemon; echo $?
# → "Error: applying store migrations to <url> — if the store is not running: docker compose up -d", exit 1, ~5s
```
Leak check after a test run:
```bash
docker exec flowspace3-db psql -U flowspace3 -tAc \
  "SELECT datname FROM pg_database WHERE datname LIKE 'fs3_migrations_%'"   # expect empty
```
As of `0a75c44`: `harness checks` 5/5, `cargo test --workspace` 0 failed, both smoke paths confirmed, no leaked databases.

## Adding a migration
Add `crates/store/migrations/000N_<what_it_does>.sql`, restart the daemon. That is the whole procedure. To undo `0002`, write `0003`.
