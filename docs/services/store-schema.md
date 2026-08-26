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
      ├──> smart_content (raw_hash, model_key) → text, text_hash, tags, extras
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
- **`Summary::extras` is persisted, in JSONB, and is NOT part of `text_hash`.**
  The type's promise is that a provider field outside `text`/`tags` is captured
  rather than dropped; a store with nowhere to put it moved the drop one layer
  down, from the wire to the database, where nothing complains. `text_hash`
  stays sha-256 of the summary TEXT alone: it is what a smart vector resolves
  through, so folding extras in would re-key every existing vector the first
  time a provider added a field — a full re-embed bought for a change to
  something that was never embedded. The consequence, stated rather than
  implied: two summaries with identical text and different extras share a
  `text_hash`, which is correct for what the digest addresses.
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
`complete_job` / `fail_job` settle it, and `retry_job(id, delay, error)` puts a
claimed row back as `pending`, due again after `delay`.

`retry_job` is a **verb, not a policy**. How many attempts are worth making and
how far apart is the worker's decision — the daemon settles it at three attempts
with backoff — and this is the one statement that decision needs. It does not
touch `attempts`, because `claim_job` already incremented it when the row was
taken: a worker deciding whether to retry reads the count it was handed rather
than one the store invented. `last_error` is recorded even though the row lives
on, which is the difference between "this is flaky" and "this is fine". Keeping
the schedule out of the store is what lets two workers with different appetites
share one queue, and it is why `fail_job` stays terminal.

`requeue_running()` is the BOOT sweep, and it exists because a worker that dies
mid-job leaves its row `running` forever. There is no lease and no heartbeat, so
nothing else can move it, and `claim_job` only looks at `pending`. The
compounding half is what makes it urgent rather than untidy: `scan_file` dedupes
on `(worktree, path)`, so a wedged row absorbs every future `add` or `scan` of
that file — `enqueue_job`'s `ON CONFLICT` bumps the payload and the deadline but
can never change the state. One `SIGKILL` during a large index would leave those
files permanently unindexable, reported as success.

It is sound only at boot, and only because fs3 has a single writer (workshop
002; PRD req 20): at that instant no worker exists to be holding a claim.
Attempts are NOT reset — `claim_job` already counted them — so a job that keeps
killing its worker stays visible as such rather than looping forever at attempt
one. A lease with an expiry is the general answer and belongs to the daemon
plan; this is the whole fix for the crash that actually happens, which is the
process stopping.

`queue_depth()` groups by `(kind, state)` rather than totalling: "142 pending
embed, 0 pending scan_file" says the scan finished and the enrichment is the
thing to wait for, while "142 pending" says nothing. `last_failure()` is the
most recent `last_error`, so a status line can say what went wrong rather than
only that something did.

**Ref layer.** `register_worktree(identity, root_path, ref_name)` is idempotent
by `(repo_id, root_path)` — `flowspace3 add` on an existing root is a re-scan
request, not a duplicate — and both inserts share one transaction, because a
repo row without its worktree is a repository fs3 believes in but cannot find,
and the next `add` would take the orphan and look like it worked.

`sync_worktree_files(worktree_id, files)` replaces the whole map and returns how
many paths vanished. The whole map rather than a delta, because the caller has
just walked the tree and knows the complete answer; the delete is scoped by an
exact `NOT (path = ANY($2))` rather than a `last_seen` sweep, which would race a
concurrent scan's writes. Deleting a pointer costs nothing derived — that is D8
working: the elements, summaries and vectors keyed by the blob survive, so
restoring the file (a branch switch, an undo) re-registers a pointer to content
that nobody has to pay for twice.

`worktree_paths_for_blob(blob)` is the reverse lookup
`worktree_files_blob_sha_idx` exists for, and the answer to the sentence above:
resolving a content hit to every live path holding it. `list_worktrees()` and
`find_worktree(root_path)` are what `status` and `scan` read; the file count is
a correlated aggregate rather than a stored column, because a cached counter is
one more thing that can be wrong.

**Filtered search.** `search_elements(model_key, query, &SearchFilters)` is
`query_embeddings` with `--repo` / `--path` / `--source` / `--min-score`
applied. The shape is the whole point: the ref-layer join lives **inside** the
neighbour CTE as an `EXISTS` predicate, so Postgres still answers
`ORDER BY vector <=> $1 LIMIT n` from the HNSW index while excluding vectors no
live path holds. Joining first and sorting after — the obvious way to write it —
reads every row in the table. Filtering after the `LIMIT` would be worse than
slow: it silently returns fewer rows than asked for.

Every filter is bound with a `NULL`-means-any guard rather than concatenated
into the statement, so there is ONE statement text whatever the caller asked
for — one plan to reason about instead of one per flag combination.

**Admin.** The control plane `flowspace3 doctor` orchestrates:
`schema_current(pool)` compares the embedded `MIGRATOR` against
`_sqlx_migrations`, `database_exists` / `create_database` answer and repair the
step before it, and `maintenance_url(url)` splits a config URL into the
`postgres`-database URL doctor connects to in order to ask about the one that is
missing. Each is one function doing one thing; doctor implements none of them.

Two decisions worth keeping: an absent `_sqlx_migrations` table is read as
"fresh database, everything missing" rather than an error, because the first run
of a new stack must not report a broken store; and `create_database` validates
the name before building a statement, because `CREATE DATABASE` takes no bind
parameters and that check is the only thing between a config URL and an
interpolated identifier.

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
- **A field added to a `fs3_core` type does not persist itself.** `Summary`
  grew `extras` with `#[serde(flatten)]`, so it deserialised correctly and
  compiled everywhere — and the store, having no column, dropped it on write
  with nothing to complain. The type's guarantee ("never dropped") had become
  false at exactly the layer that decides what survives a restart. Adding a
  field to a stored type is a migration, and the tell is a round-trip test that
  compares the WHOLE value rather than the fields you were thinking about.
- **Two decoders for one type is how a field goes missing on one path.**
  `get_smart_content` and the search join both build a `Summary`; they now share
  `summary_from_row`, because the version that did not is precisely how `extras`
  could have been fixed on the key path and stayed broken on the search path.

## Verify

```bash
docker compose up -d                     # pgvector/pgvector:pg16 on 127.0.0.1:5433
cargo test -p fs3-store                  # 46 tests: unit, admin, migrations, round-trip, flows
harness checks                           # fmt, clippy, arch (sqlx + pgvector stay in fs3-store)
```

Leak check after a run — throwaway databases drop themselves unless a test
panicked first:

```bash
docker exec flowspace3-db psql -U flowspace3 -tAc \
  "SELECT datname FROM pg_database WHERE datname LIKE 'fs3_migrations_%'"   # expect empty
```

As of `0006`'s landing: `cargo test -p fs3-store` 46 passed / 0 failed, `cargo
fmt -p fs3-store --check` clean, clippy `-D warnings` clean for `fs3-store`,
`fs3-arch-check` ok (8 crates, 70 direct edges, 0 violations), no leaked
databases. Workspace-wide `harness checks` was red at that moment for one
reason outside this crate — `cargo fmt --all --check` against
`crates/providers/examples/embed_files.rs`, another worker's in-flight file.

## What is deliberately not here

Conversations (PRD 24–27) get their own workshop. GC is a future `prune` job
kind (D8). Config is not in the database at all — it is files in
`~/.config/flowspace3/`; the database holds data only.
