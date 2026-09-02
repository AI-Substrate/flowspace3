## Problem

Production profiling attributed 261.2 of 380 DB CPU-seconds (68.7%) to `search_elements`. The old admission predicate nested a correlated `smart_content` lookup under `EXISTS(elements)`: 962,792 smart-content probes and 3,851,137 of 3,853,170 shared hits to return 40 rows. The HNSW scan itself took 12.4 ms / 1,078 buffers; the complete query took 1,667 ms and JIT compilation took 281 ms.

## Change
- Pre-resolve caller scope to raw and smart source keys once, then apply those keys inside the HNSW scan before `ORDER BY … LIMIT` (`crates/store/src/embeddings.rs:579-627`). This restores iterative-scan recovery for small repository shares without correlated `smart_content` probes.
- Bound payload and representative resolution to the current candidate page: raw candidate hashes union raw hashes reached through smart `text_hash` mappings (`:633-711`).
- Carry raw candidate count and admitted count independently (`:629-631`, `:713-715`). Stop after admitted growth stalls; at the ceiling return a short page instead of `Err` (`:820-877`).
- Return internal `SearchPage { hits, passes, candidate_limit_exhausted }`; publish only additive agent-facing metadata `scan_incomplete` and `passes`, independent of fusion and truncation.
- Disable JIT transaction-locally through the same setup function production executes (`:50-61`): measured compilation cost was 281 ms against 12 ms of vector work.

No migration, index, dependency, scoring, chunk-collapse, or expansion-constant change. The store return type and additive search metadata are the reviewer-authorized clean cutover.

## Filter matrix preserved — proof source stated honestly

The parity golden covers repository, path, raw/smart source, kind, and exact conversation at limits 10/40. Existing focused regression suites cover the remaining unchanged predicates; the table does not claim those are parity-golden cases.

| Contract | Location | Proof |
|---|---|---|
| model key, vector source, distance ceiling | `embeddings.rs:617-619` | parity covers source; existing store tests cover model/distance |
| element kinds | `:647` | parity + no-growth geometry |
| ddoc id kinds | `:648` | `pg_ddoc` regression |
| gate-open semantics, including unknown state | `:649-659` | `pg_ddoc` regression |
| ddoc schema | `:660` | `pg_ddoc` regression |
| exact conversation | `:592-603`, `:661` | parity + daemon conversation regression |
| repository, path, worktree scope source keys | `:579-627` | parity + paired 12k/5 geometry + first-light regressions |
| page-bound raw/smart mapping union | `:633-641` | smart parity + shape-loop assertion |
| deterministic shared-summary chooser | `:686-697` | parity shared-summary geometry |
| deterministic eligible element chooser | `:681-684`, `:724-729` | parity shared-raw geometry |
| collapse/provenance/final order | `:731-782` | chunk-collapse and first-light regressions |
| admitted-growth / no-error bound | `:83-107`, `:820-877` | no-growth DB test + bound decision unit test; both mutation-checked |

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
- execution: 7.322 ms;
- shared hits: 3,113;
- HNSW rows: 160; `candidate_vectors` rows: 160;
- maximum `smart_content` loops: 159, bounded by the candidate page;
- no correlated smart-content SubPlan inside candidate admission, four-field candidate target, no JIT.
- `candidate_vectors` target list remains exactly `(source_hash, source_kind, chunk_no, distance)`.

The old pathological query is never ANALYZEd in tests. The mutation uses non-ANALYZE `EXPLAIN (VERBOSE, FORMAT JSON)` and must expose both the correlated smart-content SubPlan and Materialize-over-elements shape.

## Mutation

`crates/store/tests/fixtures/search_admission_old.sql` contains the old correlated admission fragment. The static shape mutation restores it and must expose both the correlated `smart_content` SubPlan and Materialize-over-elements shape.

Review mutations were executed, then reverted:

- removing admitted-growth comparison makes `admitted_growth_stops_an_empty_content_filter_after_two_passes` red (`scan_incomplete` is not raised);
- restoring the bound error makes `filtered_search_returns_a_short_page_at_the_expansion_bound` red (`Error` vs short-page `Return`);
- removing shipped `SET LOCAL jit=off` makes `shipped_search_transaction_disables_jit_locally` red (`on` vs `off`).

## Tests

All database tests used only `flowspace3-db-test` on `127.0.0.1:5434`.

- `cargo test -p fs3-store --test search_admission` — 3/3: golden parity; 12,000 nearer foreign + five scoped returns all five on pass 1; empty content filter stops exhausted on pass 2.
- `cargo test -p fs3-store embeddings::tests` — 5/5: shape, JIT, and bound guards.
- focused store regressions — 53/53.
- focused daemon regressions — 56/56, including Rust and HTTP `scan_incomplete` carrier; the envelope publishes no duplicate exhaustion alias.

Parity golden: six predicates × two limits = twelve cases, scores within `1e-6`. It includes a raw hash shared by three elements and a summary hash shared by two raw bodies in code and conversation scopes. Predicates outside those six are covered by the named existing suites above, not claimed as golden cases.

The option-B correction is one commit on PR head `065acfd`; CI on that exact correction SHA is the gate. No local full gate is required for this review round.

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
