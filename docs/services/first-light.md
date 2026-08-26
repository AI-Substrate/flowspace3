# First light — the pipeline, wired
**Built**: 2026-08-26 (worker pij-broad-sawfish, plan 003) · **Authority**: [plan 003](../plans/003-first-light/plan.dd.md), workshops [002](../plans/prd/workshops/002-pg-schema.md) (schema), [003](../plans/prd/workshops/003-query-surface.md) (query), [004](../plans/prd/workshops/004-envelopes-and-errors.md) (envelopes) · **Code**: `crates/daemon/src/{roots,runner,scan,enrich,search,schema,status,answer}.rs`, `crates/cli/src/{doctor,client,main}.rs`, `crates/core/src/{envelope,catalog}.rs` · **Tests**: `crates/daemon/tests/first_light.rs` (11), `crates/store/tests/pg_first_light.rs` (15)

The plan called it a campfire: everything already existed as tested parts, and
this is the wiring that makes them one system. Add a path, and a question about
its code gets an answer.

```bash
docker compose up -d          # or let doctor do it
flowspace3 doctor             # engine -> stack -> database -> schema, repairing
flowspace3 daemon &           # composition root + HTTP + the worker loop
flowspace3 add /path/to/repo  # walk, hash, enqueue
flowspace3 status             # until the queue is empty
flowspace3 search "how does the queue avoid two workers taking the same job"
```

## The path a file takes

```
flowspace3 add <path>
  │
  ├─ fs3_git::repo_identity(path)      walks UP   → git:github.com/org/repo
  ├─ discovery::discover(path)         walks DOWN → 187 files, 55 refused with reasons
  ├─ fs3_git::blob_id(file)            per file   → git's own sha1
  ├─ register_worktree + sync_worktree_files      → the ref layer
  └─ enqueue scan_file  ... for every file whose blob CHANGED
                                       │
  runner::drain  claims N at a time ───┤
                                       │
  scan_file ─ get_elements(blob, parser)?  Some → SKIP (already parsed, by anyone)
            └ scan(bytes) → tree → upsert_element_tree
              └ enqueue embed(raw, batched 16) + summarize(elements ≥ 10 lines)
                                       │
  summarize ─ summarizer_for(repo).summarize(element)
            └ put_smart_content(raw_hash, model_key)
              └ enqueue embed(smart) for the summary's own text
                                       │
  embed ──── embedder_for(repo).embed(batch) → put_embeddings

flowspace3 search "<question>"
  └ embed the query with the SAME embedder → search_elements(filters) → ranked hits
```

## Decisions, and why

- **`add` never calls `fs3_git::snapshot`.** Snapshot enumerates and hashes
  everything git can see; discovery hands back what fs3 will actually parse —
  187 files rather than 3,200 on this repository. Per-file `blob_id` over the
  filtered set is both the cheaper walk and the one whose frame matches the row
  we store. It also makes a subdirectory a legal root for free: identity comes
  from walking up, paths from walking down.

- **The skip is content-addressed, not cached.** `scan_file` asks
  `get_elements(blob, parser_version)` before parsing. `Some` means these exact
  bytes have been parsed by this parser — on another branch, in another
  checkout, on another day. It is the same answer by construction, because a
  blob IS the hash of the bytes. This is what makes a re-scan of an unchanged
  tree cost zero, and it is asserted as an acceptance criterion.

- **The queue is the semaphore.** `claim_job`'s `FOR UPDATE SKIP LOCKED` hands
  N workers N different jobs, so `indexing.worker_concurrency` is the only
  concurrency number the DAEMON needs — nothing beside the queue has to agree
  with it.

  It is not, however, a number about provider parallelism, and that distinction
  is measured rather than assumed. Against Azure, in-flight requests are the
  lever: the live run did 110 embedding calls at this width, 16 texts per call,
  and both knobs bought real time because every call is a round trip. Against
  the LOCAL embedder neither does — 32 concurrent tasks against one session ran
  2.5% *slower* than sequential, and one large batch 4.4% slower than many
  small ones. What works there is a pool of independent sessions sharded by
  chunk (−40% at 16), while sharding by *file* was 12% slower than sequential,
  because per-file work spans 1 to 23 chunks and one session took half the
  corpus (pij-thorough-zakalwe, `docs/services/local-embeddings.md`).

  So a future concurrency combinator over `Arc<dyn Embedder>` cannot be one
  `max_concurrent` for both: the same number means "requests in flight" for one
  implementation and "sessions loaded" for the other. That knob belongs beside
  the provider, not here.

- **Retry is the worker's policy, not the store's.** Three attempts with
  exponential backoff, and only for failures the catalog marks `retryable`.
  That qualifier is the point: re-running a job whose cause is a missing API key
  costs three times as much and fails three times. `fail_job` stays terminal;
  `retry_job` is a verb the worker drives.

- **Enrichment is keyed by `raw_hash`** (workshop 002 D2), so the same body on
  forty branches is summarised once. A `summarize` job carries its ELEMENT
  rather than an id, so a parser bump between enqueue and claim cannot
  invalidate it.

- **`model_key` comes from `provider.key()`**, never from config. On Azure the
  config cannot tell you what served a request — only the deployment can. The
  embedder's key carries the vector WIDTH, so the key that wrote a vector and
  the key that reads it are the same string and two model spaces can never be
  compared by accident.

- **A file element its children cover earns no raw vector.** Its `raw_text` is
  the concatenation of texts already indexed one by one: the vector carried
  nothing new, was the largest in the tree to store, and — containing every
  token in the file — outranked its own functions on every question about that
  file. A file with NO children still gets one; there the file element IS the
  content, and prose would otherwise be unsearchable.

- **Filters run inside the neighbour CTE.** The ref-layer join is an `EXISTS`
  predicate beside `ORDER BY vector <=> $1 LIMIT n`, so the HNSW index still
  answers the query while excluding vectors no live path holds. Joining first
  and sorting after reads every row; filtering after the `LIMIT` silently
  returns fewer hits than asked for.

- **Score, not distance, at the boundary.** The store speaks cosine distance
  (0.0 identical); the surface speaks score (1.0 identical). `1 - distance`
  happens once, in `search.rs`, so `--min-score 0.7` is a number a human can
  reason about.

- **Every failure is an envelope with a code and a `fix`.** Including "the
  daemon is not running" — the CLI turns a transport error into
  `FS3-E-DAEMON-UNAVAILABLE` rather than letting one failure escape the
  registry. Status codes are derived from the code's spelling, so no endpoint
  ever chooses one.

- **Every db-touching endpoint runs the schema guard first**, turning
  `column "enrich" does not exist` into "the schema is 0006-0007 behind, run
  `flowspace3 doctor`". Not cached: the failure it exists to catch is precisely
  the schema changing underneath a running process. `/health` is exempt — it is
  how a CLI decides whether the daemon exists at all.

- **One binary.** The daemon ships inside `flowspace3` as `flowspace3 daemon`
  (PRD req 51): one file to install, one version, and no way for a CLI and a
  daemon of different vintages to meet. `fs3-daemon` remains the crate and the
  composition root; the CLI only starts it. `doctor` reports the daemon but
  never STARTS one — a diagnostic command that spawns a foreground process
  leaves something running the user did not ask for and cannot see.

- **`doctor` is the one CLI verb that opens a pool.** Ruled by o-prime
  2026-08-26: PRD req 20's single-writer rule governs the DATA plane, and
  doctor's writes are CONTROL plane — create the database, apply migrations —
  bootstrap operations that precede a daemon existing. It is also the verb you
  run when the daemon is DOWN, so it cannot be a client of it. It orchestrates
  `fs3_store`'s admin functions and implements none of them.

## Gotchas discovered

- **The fake embedder is 32-wide and `embeddings_1024` is not.** An offline
  stack would have indexed everything and failed at the last step. The
  composition root builds the fake at `fs3_store::EMBEDDING_DIMENSIONS` — it is
  the only place that can see both halves.
- **`claim_job` filters by kind**, so a job of an unknown kind is never claimed
  and proves nothing about the retry policy. The runner's unknown-kind arm is
  reachable only through a kind in `runner::KINDS` whose payload does not parse.
- **A relative path from an HTTP client resolves against the DAEMON's working
  directory.** The CLI canonicalises before sending, and the error message names
  the trap, because "no such path: ./src" otherwise sends the reader looking in
  the wrong place.
- **`ignore`'s `parents(true)` is what makes a subdirectory root correct** — a
  `.gitignore` ABOVE the walk root still binds. It was already set; it is worth
  knowing it is load-bearing.
- **Postgres accepts TCP before it accepts queries.** Doctor's stack step waits
  for a real connection rather than `compose ps`, because a container that is up
  but not yet answering is indistinguishable from a healthy one by `ps`.
- **`CREATE DATABASE` takes no bind parameters**, so `create_database`
  validates the name before building a statement. That check is the only thing
  between a config URL and an interpolated identifier.
- **Cost scales with ELEMENTS, not files.** 187 files on this repository is
  2,261 elements and 1,082 summarize jobs. Any estimate made from a file count
  is wrong by an order of magnitude.

## Watching it work

At the default filter the daemon streams one line per job and a progress
summary every five seconds while work is in flight:

```text
INFO fs3_daemon::runner: done kind=scan_file subject=src/auth.rs ms=91 left=1214
INFO fs3_daemon::runner: done kind=summarize subject=src/admin.rs::schema_current ms=612 left=1213
INFO fs3_daemon::runner: done kind=embed subject=16 x raw ms=104 left=1212
INFO fs3_daemon::runner: progress phase="working" scanned=18 scan_left=0 \
     summarized=54 summarize_left=44 embedded=61 embed_left=57 failed=0
```

Three decisions behind that shape:

- **The subject is the human key, not the dedupe key.** A dedupe key is an
  idempotence token — `embed:git:github.com/x:9f2c…` — and says nothing about
  what is happening to your repository. The path, the element address and the
  batch size do.
- **`left` is what makes a stream of lines a POSITION.** Every line was true
  without it and you still could not tell how far through you were; a run of
  facts with no denominator is not progress. It counts `pending` + `running`
  and excludes the job the line is reporting, so the last line reads `left=0`.
  Counted at the source per completion rather than kept as a counter, because
  the backlog GROWS while it drains — each `scan_file` enqueues the summarize
  and embed work it finds, so a decrementing counter would march to zero while
  the real backlog was still climbing.
- **Progress is derived from the QUEUE, not from counters in the loop.** A
  counter in the process would reset on restart and would not see a sibling
  worker's rows. The cost is one grouped aggregate every few seconds.

The five-second summary is emitted by the drain loop itself, not between
drains. It was between drains until 2026-08-26, and `drain` returns only when
nothing is READY — so a busy queue never left it, and the summary that exists
to narrate a long run was the one thing a long run never printed. It looked
correct in every test, because short queues empty.

Payloads are never logged. An `embed` payload carries the texts being embedded,
so dumping it would put the indexed source itself into the log, at volume, once
per batch.

## Verify

```bash
docker compose up -d
cargo test -p fs3-daemon --test first_light    # 14: the whole path + fault paths
cargo test -p fs3-store  --test pg_first_light # 16: ref layer, admin, filters
harness checks                                 # fmt, clippy -D warnings, arch, tests
```

The three tests worth reading first, because they are the claims:

- `add_scan_enrich_and_search_answer_end_to_end` — the demo, asserted.
- `a_rescan_of_an_unchanged_tree_enqueues_no_work_at_all` — the idempotence
  acceptance criterion, with summary and vector counts proven byte-identical.
- `a_behind_database_is_rejected_then_repaired_by_doctor_then_works` — the
  schema discipline as one loop.

Live-run transcript against Azure: [`first-light-run.md`](../plans/003-first-light/assets/first-light-run.md).

## What is deliberately not here

The file watcher (daemon plan — doctrine locked, not wired). `get` and `tree`
(workshop 003's companions). Text and regex modes, hybrid RRF ranking, and
span-overlap dedup — plan 003 is the semantic slice only. Conversations, MCP.
A concurrency-combinator crate: the queue already is one.
