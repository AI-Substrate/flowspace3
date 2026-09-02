# Phase 1 execution log — search admission

## tk-0101 — old-query parity golden

Added `crates/store/tests/search_admission.rs` and captured the current query's addresses and scores in `crates/store/tests/fixtures/search_admission_golden.json`. The fixture exercises limits 10 and 40 across repository, path, raw-source, smart-source, element-kind, and exact-conversation filters. It contains the two multiplicity hazards ruled by o-prime: one raw hash carried by three eligible elements, and one summary text hash mapped to two eligible raw hashes, in both code and conversation scopes.

Saved the old correlated admission fragment at `crates/store/tests/fixtures/search_admission_old.sql` for the EXPLAIN mutation check.

Evidence: `cargo test -p fs3-store --test search_admission search_parity -- --nocapture` — 1 passed. Test DB was a labeled, migrated `fs3_test_searchadmiss_*` database created beside the configured `:5433` endpoint and destroyed after capture.

## Discoveries & learnings

- **Noteworthy:** The checked-out query already projects only `(source_hash, source_kind, chunk_no, distance)` from `candidate_vectors`; o-prime amended `ac-0001` and goal 5 so the implementation asserts this existing property instead of claiming a removal.
- **Noteworthy:** Global `ddocs` is the canonical deterministic-document CLI. The builder instruction naming `node_modules/.bin/ddocs` is stale; `DL-003` records the blocking false path, and o-prime is fixing it.
- **Noteworthy:** Rust-analyzer returned zero references for exported `search_elements` despite verified callers. `DL-004` records the miss; exact-identifier search plus exact reads are the callsite proof for this packet.

## tk-0102 — bounded HNSW driver plus one-time admission

Rewrote `search_elements` so `candidate_vectors` first materializes the HNSW-ordered page capped by `candidate_limit`. Admission then joins only that bounded page against one-time relations: caller-filtered `admitted_elements`, one representative per eligible raw hash, eligible `smart_map`, one deterministic representative per summary text hash, and deduplicated raw/smart source keys. The representative CTEs preserve existential multiplicity without rescanning either relation for every candidate.

The statement now disables JIT transaction-locally: the production profile measured 281 ms of compilation against 12 ms of HNSW work, so its startup cost cannot amortize in this latency-bounded query. Candidate expansion remains unchanged and counts the pre-admission candidate page, causing selective filters to request the next larger HNSW page rather than return an under-filled result.

Added two plan-shape legs inside `embeddings.rs`:

- non-ANALYZE static proof: the `candidate_vectors` Limit estimates at most 160 rows; its child is the childless `<=>` HNSW index scan; admission joins sit above it; the old SQL mutation retains the correlated `smart_content` SubPlan and Materialize-over-elements shape;
- bounded runtime proof: on exactly 50,000 elements, 10,000 smart-content rows, and 20,000 embeddings (10,000 smart + 10,000 raw), shipped `EXPLAIN (ANALYZE, BUFFERS, VERBOSE, FORMAT JSON)` passes under `statement_timeout=30s` and parallel workers disabled, with candidate and smart-content work bounded by 160, four CTE target columns, and no JIT.

Evidence on the separate `:5434` test postmaster: `search_plan_shape_static` — 1 passed; `search_plan_shape_analyze` — 1 passed; `search_parity` — 1 passed after the rewrite.

### Incident and correction

The first mutation test wrongly ran old-query ANALYZE on the shared `:5433` postmaster; it ran 648 seconds and coincided with crash recovery. O-prime froze work, recovered the server, moved every test to the new `flowspace3-db-test` postmaster on `:5434`, and ruled that the old pathological query is non-ANALYZE forever. The permanent test now enforces that split plus a 30-second shipped-query timeout. A first join-before-kNN rewrite then timed out on the isolated postmaster; the passing design restores HNSW as the first materialized step and post-filters only its bounded page.

### Discoveries & learnings

- **Noteworthy:** PostgreSQL reports the raw HNSW Index Scan's `Plan Rows` as base cardinality; the parent `candidate_vectors` Limit carries the bounded estimate. O-prime ruled the test to assert estimated/actual rows on the Limit and actual rows on the index node.
- **Noteworthy:** Production timing AC-0005 is now load-controlled: scoped and unscoped runs are interleaved, each bracketed by open-job counts, and only runs below approximately 50 open jobs count.

## tk-0103 — parity and regressions

The first focused daemon run exposed one real regression: when the entire first HNSW page belonged to a foreign repository, post-filtering produced no rows, so `candidate_count` disappeared and the expansion loop treated the index as exhausted. The repair carries `candidate_count` independently through a single internal sentinel row when no admitted hit exists. `element_id` is nullable only on that row; Rust excludes it from results while retaining the pre-admission page count. Expansion constants remain unchanged.

Added `search_expands_after_a_fully_foreign_first_candidate_page`: forty nearer foreign vectors fill the initial page for limit 10; ten farther scoped vectors must all return after expansion.

Final focused receipts on the separate `:5434` postmaster:

- `cargo test -p fs3-store --test search_admission` — 2/2;
- `cargo test -p fs3-store search_plan_shape` — 2/2;
- `cargo test -p fs3-store --test pg_first_light --test pg_ddoc --test pg_store_flows` — 53/53;
- daemon `conversation_query`, `first_light`, `oversize`, `search_empty`, `search_lexical`, and `search_scope_starvation` — 55/55.

These cover the full caller filter matrix, deterministic shared-summary choice, exact conversation pin, ddoc state/schema/id filters, distance ceiling, chunk collapse/best score, candidate expansion, and semantic-only scoped starvation.

## Full-gate diagnosis after main merge

Merged main commit `f73dee0` (plan 012's process-wide database-mutation permit) before rerunning the full suite. The first captured suite failed only in unrelated `streaming::progress_is_reported_while_the_queue_is_still_draining`; that test passed three consecutive correctly configured targeted runs, classifying the full-suite miss as environmental timing under load. No plan-013 code touches provider progress reporting.

The merge exposed a real fixture boundary in `search_plan_shape_analyze_bounds_admission_work`: with only 10,000 equal smart vectors, PostgreSQL could choose either HNSW or sequential sort at nearly equal cost. Adding 10,000 raw vectors stabilizes the prod-shaped 20,000-vector fixture without weakening any assertion. Both shape legs now pass together after the merge: 108.468 ms, 2,691 shared hits, HNSW/candidate rows 160, smart-content max loops 1.
