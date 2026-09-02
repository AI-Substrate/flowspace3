# search-admission gate failure classification

Gate head: `6294e9afba69ab3ad687b35c19dcc721fbbb9f0b` (main merged, including #95 database-mutation permit). Exclusive slot retained.

## Exact failures

1. Direct full-suite capture at `.harness/temp/agent/search-admission-suite.log` failed only at `fs3-daemon --test streaming::progress_is_reported_while_the_queue_is_still_draining` (`crates/daemon/tests/streaming.rs:199`): expected one provider line, observed zero. This is outside plan 013's fence and search path; classify as an unrelated environment/timing failure, not a 013 regression. I will not modify it.
2. Targeted diagnosis found `fs3_store::embeddings::tests::search_plan_shape_analyze_bounds_admission_work` nondeterministic on the merged head: static non-ANALYZE chose HNSW and passed, while ANALYZE on an independently seeded 10k-vector scratch DB chose a Seq Scan and failed “no HNSW vector driver.” Execution was 142.058 ms, so this is a 013 fixture/planner-cost threshold, not an infrastructure connection fault. Production behavior remains unclaimed until post-bounce proof.

## Repair

Increase only the shape fixture from 10k to 20k embeddings (10k smart + 10k raw; elements remain 50k, smart_content remains 10k) so sequential-sort cost is no longer tied with HNSW cost. Keep all binding assertions and the 30-second timeout unchanged. Run targeted static+ANALYZE repeatedly through the normal test command, then re-run the captured suite and full gate.

## Resolution

- `streaming::progress_is_reported_while_the_queue_is_still_draining` passed three consecutive correctly configured targeted runs. Classification: environmental timing flake under the first full-suite load; no plan-013 code touches progress/provider logging.
- The stabilized shape fixture passed both legs together: 108.468 ms, 2,691 shared hits, HNSW/candidate rows 160, smart-content max loops 1.
