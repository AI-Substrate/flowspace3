# Search latency telemetry

Every search records where its time went. Status and doctor keep percentiles over recent searches, so a slow search can be measured without a bench.

## What it is

- **`meta.timing` on every `/search` envelope** (`SearchTiming` in `crates/core/src/views/search.rs`). All values are whole milliseconds.
  - The serial stages are `guard`, `scope`, `embed`, `store`, `rank`, `agents` and `diagnose`. They add up to roughly `total_ms`.
  - `store_ms` is the wall time of the four store legs. They run concurrently, so it is the slowest leg's time, not their sum. Each leg is also reported on its own:
    - `semantic_ms` is the scoped vector leg. It comes with `semantic_acquire_ms`, `semantic_pass_ms[]` (one entry per candidate pass) and `candidates` (the final candidate limit).
    - `lexical_ms` is the scoped keyword leg. It comes with `lexical_acquire_ms`.
    - `wide_semantic_ms`, `wide_lexical_ms` and `wide_passes` are the unscoped legs behind the "other repositories" hint. They appear only when those legs ran.
- **One `search timing` log line per search,** at info level: total, embed, store, semantic, lexical, wide, passes, candidates and repo. A search over 5 s also logs the `slow search` warning, which now carries the per-leg split.
- **`status.search_latency`:** p50, p95 and max per stage over the last 1,024 searches, plus `slow` (the number over `slow_ms`, 5 s) and `slowest_stage` (the stage with the highest p95, excluding total and store). It is kept in memory, so it starts empty at every daemon restart.
- **A `search` row in doctor:**
  - `ok` while p95 total is at most 5 s.
  - `warn` above that. The row names the slowest stage and what to check for it.
  - `info` when status cannot be read.

## Decisions

- **Acquire time is measured apart from query time.** The pool has 8 connections, shared with every background worker, and one search holds up to 4 of them at once. So a slow leg can be waiting for a connection rather than running a slow query, and the split shows which.
- **Per-pass times:** the scoped vector statement rebuilds its scope on every candidate pass, so a slow semantic leg is often several medium passes rather than one bad one.
- **`meta`, not `data`:** timing is a fact about this answer, like `passes`, and not a result.
- **In memory, not in the database:** the record is cheap and has no write path or schema. The long view belongs to the search bench.

## Gotchas

- Scope preparation happens inside the vector statement, so it is not yet a stage of its own. It is part of each `semantic_pass_ms` entry.
- Only searches that succeed through `/search` are recorded. `ask`'s inner searches and failed searches are not.

## The bench (`search-bench`)

The before/after gate for search-speed changes is an example of the daemon crate, so its database access stays a dev-only edge:

```bash
# A synthetic corpus shaped like prod (scale 1 ≈ 1.6M vectors), on the bench server only.
cargo run --release -p fs3-daemon --example search-bench -- generate \
    --server postgres://flowspace3:flowspace3@127.0.0.1:5435 --name bench_1x --scale 1

# Cold and warm searches through the daemon's own search path, with per-stage p50/p95.
cargo run --release -p fs3-daemon --example search-bench -- run \
    --db postgres://flowspace3:flowspace3@127.0.0.1:5435/bench_1x \
    --arm baseline --arm ef100:hnsw.ef_search=100 --out /tmp/bench.json

# The installed CLI against its configured daemon (read-only), reading meta.timing.
cargo run --release -p fs3-daemon --example search-bench -- e2e \
    --queries crates/daemon/examples/search-bench/queries/code-questions.txt --cwd .
```

- **The corpus** is synthetic: no real text or vector.
  - Its shape (repositories, worktrees, blobs, conversations, turns, elements per kind, text lengths and turn classes) and its vector similarity statistics come from in-database aggregates of the prod index.
  - Scale `n` repeats the repositories `n` times under new identities: the index grows by more repositories, and each scope becomes a smaller share of it.
- **Cold and warm:**
  - every query text is new, so the bench embedder places it at a fresh point and the first search is cold;
  - the immediate repeat is warm;
  - arms (`name:setting=value,…`, applied as connection settings) take turns going first, so neither always pays for the cold cache, and each arm gets its own fresh queries;
  - per-search buffer hits and reads come from `pg_stat_statements`.
- **Stages** come from each search's `SearchTiming`, so the bench and prod's `status` measure the same thing.
- **Limits** (o-prime, 2026-10-14):
  - corpora live on the bench server `flowspace3-db-bench` (:5435, 10 GB memory cap);
  - at least 120 GB must stay free in the Docker VM;
  - float32 corpora go up to 3x only;
  - drop a scale database when you are done with it.

## Verify

```bash
flowspace3 search "how does a worker claim the next pending job" --json | jq .meta.timing
flowspace3 status --json | jq .data.search_latency
flowspace3 doctor            # the `search` row
cargo test -p fs3-daemon --test search_timing
```

## Code

- `crates/daemon/src/search_latency.rs`: the ring and the percentiles
- `crates/daemon/src/search.rs`: the leg and stage timings
- `crates/daemon/src/http.rs` (the `search` handler): guard, scope, total, the log line and recording
- `crates/store/src/embeddings.rs` (`SearchPage`) and `crates/store/src/lexical.rs` (`LexicalPage`): acquire and pass timings
- `crates/cli/src/doctor.rs` (`search_row`) and `crates/cli/src/render/surfaces/status.rs` (`search_line`)
