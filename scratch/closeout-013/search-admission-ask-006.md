# search-admission STOP-and-ask 006 — rewritten shipped query exceeds 30-second bound

## Red verdict

After o-prime fixed `:5434` maintenance access, the focused shape test created and migrated its own `fs3_test_searchplan_*` database on the separate test postmaster, seeded exactly 50,000 elements, 10,000 smart-content rows, and 10,000 smart embeddings, and set transaction-local:

- `statement_timeout = '30s'`
- `max_parallel_workers_per_gather = 0`

The **shipped rewritten query's** `EXPLAIN (ANALYZE, BUFFERS, VERBOSE, FORMAT JSON)` timed out. The old pathological query was not reached and was never executed. The helper closed the pool and dropped its scratch database before surfacing the red verdict. No production/shared `:5433` work occurred.

Command result: `canceling statement due to statement timeout`; focused test failed after 35.78 seconds.

## Current conclusion

The current admitted-elements/smart-map/admitted-sources join rewrite does not satisfy the performance contract. The test produced no completed plan, so whether it lost HNSW ordering is not yet evidence.

## Ruling needed

May I proceed by running only bounded, non-ANALYZE `EXPLAIN (VERBOSE, FORMAT JSON)` for the rewritten query on the separate `:5434` scratch corpus, inspect the chosen shape, and redesign until the cheap plan retains `embeddings_1024_vector_idx`; then rerun the one 30-second ANALYZE tripwire? I will not raise the timeout or weaken the 50k/10k corpus.

No further code or database work until ruled.
