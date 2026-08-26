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

`fs3_store::migrate` is called once, from `crates/daemon/src/main.rs`, after
config and before serving. The daemon is the single writer, so startup is the
only moment migrations can run without racing anyone. sqlx records what it has
applied in a `_sqlx_migrations` table and skips those, so running it every boot
is a no-op — proven in `crates/store/tests/pg_migrations.rs`.

If migration fails the daemon **exits nonzero** naming `database.url` and
`docker compose up -d`. A writer that cannot reach its own schema has nothing
useful to serve, so it says so once at boot rather than failing every request.

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
    pub db: PgPool,
    pub config: Config,
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
