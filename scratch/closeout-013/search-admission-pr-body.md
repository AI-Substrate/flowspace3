## Problem

Production profiling attributed 261.2 of 380 DB CPU-seconds (68.7%) to `search_elements`. The old admission predicate nested a correlated `smart_content` lookup under `EXISTS(elements)`: 962,792 smart-content probes and 3,851,137 of 3,853,170 shared hits to return 40 rows. The HNSW scan itself took 12.4 ms / 1,078 buffers; the complete query took 1,667 ms and JIT compilation took 281 ms.

## Change

- Materialize the HNSW-ordered `candidate_vectors` page first, bounded by `candidate_limit` (`crates/store/src/embeddings.rs:505-513`).
- Resolve caller-eligible elements once (`:519-557`), collapse them to one deterministic representative per raw hash (`:559-562`), map eligible smart content once (`:564-575`), and build deduplicated raw/smart source keys (`:577-582`).
- Post-filter only the bounded candidate page against those keys (`:584-589`), then resolve smart and element representatives without correlated probes (`:591-604`).
- Carry pre-admission `candidate_count` through an internal nullable sentinel when a page admits zero rows (`:611-656`, `:742-770`). This preserves the existing expansion constants and fixes the 100%-foreign-first-page case.
- Disable JIT transaction-locally for this latency-bound statement (`:50-54`, `:704-706`): measured compilation cost was 281 ms against 12 ms of vector work.

No migration, index, dependency, public API, scoring, chunk-collapse, or expansion-policy change.

## Filter matrix preserved

| Contract | Location |
|---|---|
| embedding model, vector source, distance ceiling | `embeddings.rs:509-511` |
| element kinds | `:522` |
| ddoc id kinds | `:523-524` |
| gate-open semantics, including unknown state | `:525-535` |
| ddoc schema | `:536-537` |
| exact conversation address prefix | `:538-539` |
| repository, path, worktree through live files | `:540-549` |
| repository, path, worktree through conversations | `:550-557` |
| deterministic shared-summary chooser | `:564-575` |
| deterministic eligible element chooser | `:559-562`, `:599-604` |
| live/conversation provenance selection | `:621-643` |
| nearest-per-element collapse and final order | `:606-656` |

## EXPLAIN before / after

**Before — production reference query:**

- execution: 1,667.423 ms;
- `smart_content` loops: 962,792;
- shared hits: 3,853,170 total, 3,851,137 from the smart-content probe;
- spilling `Materialize` over 86,191 `elements` rows;
- JIT: 281.374 ms;
- HNSW: 12.4 ms / 1,078 buffers.

**After — isolated `:5434` prod-shaped fixture:**

- seed: 50,000 `elements`, 10,000 `smart_content`, 20,000 embeddings (10,000 smart + 10,000 raw);
- execution: 108.468 ms;
- shared hits: 2,691;
- HNSW rows: 160; `candidate_vectors` rows: 160;
- maximum `smart_content` loops: 1;
- no Materialize-over-elements, no correlated smart-content SubPlan, no JIT;
- `candidate_vectors` target list remains exactly `(source_hash, source_kind, chunk_no, distance)`.

The old pathological query is never ANALYZEd in tests. The mutation uses non-ANALYZE `EXPLAIN (VERBOSE, FORMAT JSON)` and must expose both the correlated smart-content SubPlan and Materialize-over-elements shape.

## Mutation

`crates/store/tests/fixtures/search_admission_old.sql` contains the old correlated admission fragment. The static shape test replaces the new post-admission join with that fragment. The shipped plan must retain a childless `<=>` HNSW driver under the 160-row `candidate_vectors` Limit; the old mutation must fail by restoring the correlated SubPlan and Materialize shape.

## Tests

All database tests used only `flowspace3-db-test` on `127.0.0.1:5434`.

- `cargo test -p fs3-store --test search_admission` — 2/2.
- `cargo test -p fs3-store search_plan_shape` — 2/2.
- `cargo test -p fs3-store --test pg_first_light --test pg_ddoc --test pg_store_flows` — 53/53.
- daemon `conversation_query`, `first_light`, `oversize`, `search_empty`, `search_lexical`, `search_scope_starvation` — 55/55.

Parity golden: limits 10 and 40 across repository, path, raw source, smart source, kind, and exact conversation filters; scores within `1e-6`. It includes one raw hash shared by at least three elements and one summary hash shared by at least two raw bodies in both code and conversation scopes. A separate regression makes the entire first HNSW page foreign-repository content and still requires the full scoped limit.

Full `harness checks`: queued for the exclusive gate slot.

## Production follow-up

After o-prime merges and bounces:

1. Re-run the production reference EXPLAIN read-only: smart loops <1,000, shared hits <100,000, execution <300 ms.
2. Interleave scoped and unscoped timings, recording `flowspace3 status` open-job count immediately before and after every run.
3. Count only runs below approximately 50 open jobs: main-checkout query <1 s ×3; prior timeout queries <2 s; unscoped query from `/Users/jordanknight/substrate/chainglass` <5 s.

## Assumptions

- `search_elements` remains the daemon semantic-search entry point; lexical fusion order is unchanged.
- Candidate expansion owns selective-scope recovery; admission must preserve the pre-admission page count even when no hit survives.
- Multiple eligible elements for one raw hash and multiple eligible raw hashes for one summary are existential, not multiplicative; deterministic representatives preserve the old ranking/output contract.
- O-prime owns merge, bounce, and production measurements. This branch does not touch production state.
