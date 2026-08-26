# Store schema
**Built**: 2026-08-26 (worker pij-musical-sylac, w-schema) · **Authority**: [workshop 002](../plans/prd/workshops/002-pg-schema.md) · **Code**: `crates/store/src/{elements,smart,embeddings,jobs}.rs`, `crates/store/migrations/0003–0005` · **Tests**: `crates/store/tests/pg_store_flows.rs`, `pg_round_trip.rs`, `pg_migrations.rs`

The shape of the data and the typed API over it — three layers, one queue, and
one rule that explains most of the design: **enrichment is addressed by the hash
of the text it describes, never by a row id.**

How to change the schema at all: [`database-migrations.md`](database-migrations.md).

## The map

```
REF LAYER (cheap pointers, per checkout)
  repos ──< worktrees ──< worktree_files (worktree_id, path) → blob_sha
                                                                  │
CONTENT LAYER (shared by every branch and repo that holds the bytes)
  elements  (blob_sha, parser_version, address, span_start)  ──┘
      │ raw_hash = sha256(raw_text)          the dirtiness key
      ├──> smart_content (raw_hash, model_key) → text, text_hash, tags
      │                                            │
      └──> embeddings_1024 (source_hash, source_kind, model_key) → vector(1024)
             source_kind='raw'   → source_hash = elements.raw_hash
             source_kind='smart' → source_hash = smart_content.text_hash

JOB BACKLOG
  jobs (kind, dedupe_key, payload, state, priority, not_before, attempts)
```

Nothing in the content layer points at the ref layer, and nothing in the ref
layer points at the content layer. They meet at `blob_sha`, which is a value,
not a foreign key. That is what lets forty branches holding one file share one
parse and one summary.

## Key decisions and why

- **The store is a derived cache, and says so.** Every element row is
  reproducible by re-scanning its blob. This is why migration 0004 DROPs 0001's
  `elements` table instead of ALTERing it: 0001 had no `parser_version`, no
  parent link, no `raw_hash` and no `enrich` verdict, so three of the new
  columns could only have been backfilled by inventing them. The cost of the
  drop is one re-scan.
- **Enrichment keyed by `raw_hash`, never `element_id`** (workshop D2). The same
  method body on forty branches is summarised ONCE. "Dirty" is the absence of a
  `smart_content` row, not a stored flag — so nothing can drift out of sync with
  reality, and a model bump is a new `model_key` that leaves the old rows intact
  (rollback is instant).
- **`(address, span_start)` identifies an element, not `address` alone.** The
  scanner emits `struct Rect` and `impl Rect` as two elements at one address. The
  workshop's original key would have collapsed the pair into one row on every
  scan — a data-loss bug that looks exactly like a successful scan.
- **`smart_content.text_hash` exists so summary vectors can be resolved.** The
  workshop defines a smart embedding's `source_hash` as `sha256(smart text)`, and
  nothing recorded that digest, so a nearest-neighbour hit on a summary had no
  path back to its element. It is computed in Rust by `fs3_core::content_hash`,
  not by a Postgres expression: one hash function in the system, not two.
- **One table per vector width** (workshop D3). HNSW needs a typed dimension; an
  untyped `vector` column cannot be indexed, which would make every search a
  sequential scan. A 1536-wide model arrives as `embeddings_1536` in a new
  migration.
- **No FKs from enrichment to elements** (workshop D7). Enrichment outlives any
  one parse. Collection is an explicit future `prune` job (D8), never a cascade —
  a worktree being removed must never delete something an LLM was paid for.
- **The queue IS the dirty-file list** (workshop D1). No `dirty_files` table: a
  dirty file is a pending `scan_file` job, and the debounce is the `not_before`
  column rather than a timer in the daemon.
- **`raw_text` is stored inline.** Workshop open question 2, resolved to the fs2
  precedent: a query resolves content without needing repo access. The DB grows
  with the code it indexes, which is the accepted cost.

## The flows (what the API is shaped around)

The API is one function per flow, not CRUD per table, and speaks `fs3_core`
types throughout — there is no DTO layer between crates.

**Scan.** `get_elements(blob, parser_version)` → `None` means nobody has parsed
these bytes with this parser, on any branch; that is the signal to do the work.
`Some` is the skip. `upsert_element_tree(blob, parser_version, root, enrich)`
writes the whole tree in one transaction, assigning parent links as it descends.
`enrich` is the scanner's injected policy (D5) — the store records the verdict,
it never computes it.

Because a blob is the hash of the bytes, the key set for a given
`(blob, parser_version)` cannot change between runs. There is no stale row to
reconcile and no reconciling delete to write.

**Enrich.** `missing_enrichment(model_key, limit)` is the D6 reconciler sweep:
elements marked `enrich` with no `smart_content` row for that model, deduplicated
by `raw_hash` so one body is one piece of work no matter how many branches hold
it. Deriving the backlog from the schema rather than trusting the queue is what
makes crashes, model changes and policy changes all converge without a manual
replay. `put_smart_content(raw_hash, model_key, &Summary)` stores the answer.

**Search.** `put_embeddings(model_key, &[NewEmbedding])` writes vectors;
`query_embeddings(model_key, query, limit)` returns `SimilarElement`s
nearest-first. The neighbour search finishes — `ORDER BY … LIMIT` inside a CTE —
*before* anything is joined, which is what lets the HNSW index answer it; joining
first and sorting after reads every row.

Two resolutions after that, each picking one representative on purpose: a smart
vector resolves through `text_hash` to the raw hash it describes, and a raw hash
resolves to the lowest-id element that has it. The same body exists at many
`(blob, parser_version, address)` triples — that sharing is the point of D2 — so
one is an example, not the answer. Resolving a hit to every live path that holds
it is the ref layer's job.

**Queue.** `enqueue_job(kind, dedupe_key, payload, delay)` is an upsert against a
PARTIAL unique index covering only `pending` and `running` rows. A watcher firing
five times for one save gets one row whose `not_before` is pushed out each time —
the debounce, in SQL. A `done` or `failed` job leaves the index, so the next edit
to that file queues freely.

`claim_job(&[kinds])` is the D4 pattern:

```sql
UPDATE jobs SET state = 'running', attempts = attempts + 1, updated_at = now()
 WHERE id = (SELECT id FROM jobs
              WHERE state = 'pending' AND not_before <= now() AND kind = ANY($1)
              ORDER BY priority DESC, not_before
              FOR UPDATE SKIP LOCKED
              LIMIT 1)
RETURNING id, kind, dedupe_key, payload, attempts
```

`SKIP LOCKED` is the whole point: a row another worker is mid-claim on is stepped
over rather than waited on, so N workers polling together get N different jobs and
none of them block. That is what lets an LLM job and an embedding job run at once.
`complete_job` / `fail_job` settle it.

## Gotchas discovered

- **Running the store suite used to migrate the SHARED database.** With an
  unpushed `0003` in the working tree, every other worker's `cargo test` failed
  `VersionMissing(3)` against a tree that did not contain it — and after a
  migration file was edited, `VersionMismatch` instead, which does not clear by
  itself. Every store test now takes a throwaway database. **Never apply an
  unpushed migration to the 5433 stack.** If it happened anyway, the reset is
  `docker exec flowspace3-db psql -U flowspace3 -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"`.
- **`ON CONFLICT` against a PARTIAL unique index needs the predicate.**
  `ON CONFLICT (dedupe_key) WHERE state IN ('pending','running')` — without the
  `WHERE`, Postgres cannot infer the index and rejects the statement.
- **An HNSW index is built for ONE operator class.** `vector_cosine_ops` answers
  `<=>` and nothing else; a query written with `<->` silently gets a sequential
  scan.
- **The two `model_key` columns are different namespaces.**
  `smart_content.model_key` names the summarising model, `embeddings_*.model_key`
  the embedding model. Joining them to each other looks natural and is wrong.
- **`sqlx` has no date/time feature enabled here**, deliberately. Timestamps are
  written by SQL (`now()`, `make_interval(secs => $n)`) and read as `::text` when
  they are needed at all, so no chrono/time type crosses the boundary.
- **A tree write must be one transaction.** A half-written tree is worse than
  none: `get_elements` would report the blob as parsed and hand back a truncated
  shape, and the scan flow's skip would make that permanent.
- **`raw_hash` is stored but never read back.** `Element::new` re-derives it from
  `raw_text`, which is what makes "the hash changed" mean "the text changed";
  trusting the stored copy would let a wrong row pass itself off as right.

## Verify

```bash
docker compose up -d                     # pgvector/pgvector:pg16 on 127.0.0.1:5433
cargo test -p fs3-store                  # 24 tests: unit, migrations, round-trip, flows
harness checks                           # fmt, clippy, arch (sqlx + pgvector stay in fs3-store)
```

Leak check after a run — throwaway databases drop themselves unless a test
panicked first:

```bash
docker exec flowspace3-db psql -U flowspace3 -tAc \
  "SELECT datname FROM pg_database WHERE datname LIKE 'fs3_migrations_%'"   # expect empty
```

As of `2dd5a25`: `cargo test -p fs3-store` 24 passed / 0 failed, `cargo fmt -p
fs3-store --check` clean, clippy `-D warnings` clean for `fs3-store`,
`fs3-arch-check` ok (8 crates, 63 direct edges, 0 violations), no leaked
databases. Workspace-wide `harness checks` could not be run green at that commit
for reasons outside this crate: `crates/cli/src/show.rs` was mid-cutover against
a renamed `fs3_core` config type, and `cargo clippy` was resolving a rustup shim
reporting rustc 1.85 while the toolchain is 1.95 — both were other workers'
in-flight changes, both reported to o-prime.

## What is deliberately not here

Conversations (PRD 24–27) get their own workshop. GC is a future `prune` job
kind (D8). Config is not in the database at all — it is files in
`~/.config/flowspace3/`; the database holds data only.
