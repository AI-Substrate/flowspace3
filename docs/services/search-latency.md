# Search latency telemetry

Every search records where its time went. Status and doctor keep percentiles over recent searches, so a slow search can be measured without a bench.

## What it is

- **`meta.timing` on every `/search` envelope** (`SearchTiming` in `crates/core/src/views/search.rs`). All values are whole milliseconds.
  - The serial stages are `guard`, `scope`, `embed`, `store`, `rank`, `agents` and `diagnose`. They add up to roughly `total_ms`.
  - `store_ms` is the wall time of the four store legs. They run concurrently, so it is the slowest leg's time, not their sum. Each leg is also reported on its own:
    - `semantic_ms` is the scoped vector leg. It comes with `semantic_acquire_ms`, `semantic_pass_ms[]` (one entry per candidate pass) and `candidates` (the final candidate limit).
    - `lexical_ms` is the scoped keyword leg. It comes with `lexical_acquire_ms`.
    - `wide_semantic_ms` and `wide_passes` are the unscoped vector leg behind the "other repositories" hint. They appear only when that leg ran and was waited for; `wide_dropped` says the answer went without it.
    - The hint's keyword page comes from the scoped keyword leg's own pass (one trigram scan yields both pages), so its cost is part of `lexical_ms`. `wide_lexical_ms` is no longer reported.
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
