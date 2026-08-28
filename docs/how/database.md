# Database — changing the schema, and using it from a service

Postgres with pgvector, one database, one writer. Migrations are plain numbered
`.sql` files that ship inside the binary. You never wipe the database to change
it.

## Changing the schema

Add one file. That is the whole procedure.

```
crates/store/migrations/0002_edges.sql
```

```sql
CREATE TABLE IF NOT EXISTS edges (...);
```

Then restart the daemon — it applies pending migrations at boot. Done.

The rules that keep this simple:

- **Forward-only.** There are no down migrations. To undo `0002`, write `0003`.
- **Never edit an applied file.** sqlx checksums each migration; changing one
  that has already run makes the daemon refuse to start, on purpose. Fixing a
  mistake is another numbered file.
- **Never wipe the database.** If you find yourself reaching for
  `docker compose down -v`, the migration you need is the answer instead.
- **Number in sequence**, four digits, lower_snake description:
  `0002_edges.sql`. The number is the version; the description is for humans.

## How it works

`crates/store/src/lib.rs` embeds the directory at compile time:

```rust
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn migrate(pool: &PgPool) -> Result<(), StoreError> { ... }
```

Embedded means the shipped binary carries its own schema history — there are no
migration files to deploy, and no way for the binary and the files to disagree.

`fs3_store::migrate` is called once, from `serve()` in
`crates/daemon/src/main.rs`, after config and before serving. The daemon is the
single writer, so startup is the only moment migrations can run without racing
anyone. sqlx records what it has applied in a `_sqlx_migrations` table and skips
those, so running it every boot is a no-op — proven in
`crates/store/tests/pg_migrations.rs`.

If migration fails the daemon **exits nonzero**, naming the database it tried
(password redacted) and `docker compose up -d`. A writer that cannot reach its
own schema has nothing useful to serve, so it says so once at boot rather than
failing every request. `crates/daemon/tests/boot_contract.rs` holds that
promise: nonzero exit, the database named, the fix named, the password absent.

After boot the pool is lazy again: `GET /health` keeps answering through a
database outage, so `flowspace3 ping` still tells "daemon down" from
"database down".

## Using the database from a service

There is **no repository port and no store trait** — Postgres is a requirement,
not a variable ([workshop 001](../plans/001-fs3-foundations/assets/workshops/001-architecture.md)
rule 3 refuses the abstraction, and names "repository-trait over sqlx" as a
refused anti-pattern). The store is concrete, and you talk to it directly.

The whole injection story:

```rust
// 1. The composition root builds ONE pool, from config.   (wiring.rs)
pub struct AppState {
    pub embedder: Arc<dyn Embedder>,
    pub summarizer: Arc<dyn Summarizer>,
    pub db: PgPool,          // <- the one pool
    pub config: Config,
    // ...plus private per-repo provider overrides
}

// 2. A handler or worker takes the state and hands the pool down.
async fn index(State(state): State<AppState>) -> ... {
    fs3_store::upsert_element(&state.db, &element).await?;
}

// 3. A store function takes &PgPool. That is the entire interface.
pub async fn upsert_element(pool: &PgPool, element: &Element) -> Result<(), StoreError>
```

A new service that needs the database adds **a parameter**, not a port, not a
constructor, not a registration. `PgPool` is an `Arc` internally, so cloning it
is cheap and `AppState` is `Clone` — axum clones it per request already.

`sqlx` stays inside `fs3-store`. Everything else, the daemon included, gets
`PgPool` through the store's re-export (`fs3_store::PgPool`) and reaches the
database through store functions. The architecture check enforces this: adding
`sqlx` to another crate's dependencies fails `harness checks`.

## Running it

```bash
docker compose up -d          # pgvector/pgvector:pg16 on 127.0.0.1:5433
cargo run -p fs3-daemon       # migrates, then serves
```

Store tests run against that same stack and **fail rather than skip** when it is
down, naming the command. `FS3_TEST_DATABASE_URL` points them elsewhere.

## Hand-run daemon isolation

Never point a development daemon at a shared test or production database. The
supported recipe owns every spend-bearing and stateful seam:

```bash
flowspace3 daemon --sandbox
```

The command creates a unique migrated child database, loads only its own minted
configuration, forces fake embedding, summarization, and agent providers,
reserves a free loopback port, and prints its ready line only after wiring,
bind, and key publication succeed. SIGINT and SIGTERM stop new dequeueing and
drop the child database; a second signal cancels remaining in-flight work but
still runs cleanup. If cleanup fails, the exit names the database and prints a
host-tool-independent fallback using `docker exec flowspace3-db psql ...`.

The former manual four-seam recipe—empty config directory, disposable database,
unique daemon port, and fake provider selections—is retained only as the
appendix in `flowspace3 docs get daemon`. Use it to diagnose sandbox boot, not
as the normal isolation path.

A different future posture will run real providers against a real **read-only**
index for chat-only verification. It is intentionally separate: provider
selection alone cannot disable add, scan, enrichment, or background writes.
That capability boundary must be enforced before a sibling sandbox flag ships.

## Which database a test may touch

Two rules, because the production database has been written to by a test run
twice (ruling: `.harness/government/rulings/2026-08-27-production-database.md`).
Both refuse by default; neither has a fallback.

1. **A test that opens a pool** gets its URL from
   `fs3_testkit::test_database_url()`, which panics unless
   `FS3_TEST_DATABASE_URL` says so out loud. The URL cannot be the
   discriminator — CI sets it to the shipped default, where that address names
   a disposable service container — so what is asked for is that somebody
   decided on purpose.
2. **A test that spawns `flowspace3`** builds its command with
   `fs3_testkit::sealed(binary, config_dir, TestDatabase::…)`, which scrubs
   every inherited `FS3_*` and pins both the config directory and the database.
   Rule 1 does not cover this: a subprocess opens its own pool, from its own
   configuration, and with no `[database]` section to find it resolves
   `DatabaseConfig::DEFAULT_URL` — the shipped address, which on a developer
   machine is the real store. Daemon boot migrates before it serves, so that is
   a production write. `crates/testkit/tests/spawn_isolation.rs` fails the build
   if a test constructs such a command by hand.

Four things enforce this without being asked: the `testdb` and `prodguard`
gates in `harness checks`, the daemon's own refusal to boot when a test marker
is present and no layer chose a database, and `fs3-test-suite`. The runner uses
the configured URL only to select a server, mints a migrated
`fs3_test_<epoch>_<entropy>` database for that one run, injects it into every
test binary, and drops it afterward. Concurrent worktrees therefore cannot
share application data by accident. A killed runner leaves a timestamped,
visible orphan; the next run drops `fs3_test_*` databases older than the named
six-hour threshold and prints the policy plus every swept name.

The Postgres cluster remains shared. A backend crash or cluster recovery can
still kill connections across every isolated database. `harness checks`
classifies connection-shaped output as infrastructure failure, never as a PASS
and never as an ordinary assertion failure.
