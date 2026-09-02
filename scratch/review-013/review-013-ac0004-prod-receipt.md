# review-013 — ac-0004 prod EXPLAIN receipt (pre-bounce)

**Taken**: 2026-09-02, prod `flowspace3-db` `:5433`, database `flowspace3`.
**Statement**: the SHIPPED `SEARCH_ELEMENTS_SQL` at `8d04a77` (byte-identical to `b332f46`; the final commit changed only `expansion_decision` and its unit test).
**Authorisation**: o-prime prime-reply-001 item 2(b) — one EXPLAIN ANALYZE per statement shape, `BEGIN READ ONLY`, `statement_timeout 30s`, no parallel, load1 < 15, no retention purge in progress.

## Conditions at run time — all met

- load1 **10.51** (`10.51 12.59 14.79`), below the ceiling of 15. Unchanged post-run.
- `flowspace3 status` queue **0**; last retention purge `06:05:54Z`, hours earlier — none in progress.
- Transaction: `BEGIN READ ONLY` / `SET LOCAL statement_timeout='30s'` / `SET LOCAL max_parallel_workers_per_gather=0` / `SET LOCAL hnsw.iterative_scan=strict_order` / `SET LOCAL jit=off` / **one** `EXPLAIN (ANALYZE, BUFFERS, VERBOSE, FORMAT JSON)` / `ROLLBACK`. Nothing written. The old statement was never executed on prod.

## Result — every ac-0004 criterion passes, with margin

| Criterion | Threshold | Measured | Margin |
|---|---|---|---|
| Execution time | < 300 ms | **35.068 ms** | 8.6× |
| Shared buffers (hit+read) | < 100,000 | **8,063** (hit 7,709 / read 354) | 12.4× |
| `smart_content` max Actual Loops | < 1,000 | **1** | — |
| HNSW still the driver *(reviewer clause, adopted)* | childless, `<=>`-ordered, rows ≤ 160 | **rows 160, children 0, `<=>` yes, loops 1** | pass |

Supporting shape, all as promised: correlated `smart_content` SubPlans **0**; `Materialize`-over-`elements` **0**; `candidate_vectors` target list exactly 4 columns (`source_hash, source_kind, chunk_no, distance`) with no vector; **no JIT** node. The three `smart_content` index-scan nodes run at loops 0 / 1 / 1. Planning time 1.815 ms.

## Against the before-numbers

| Baseline | Before | After | Change |
|---|---|---|---|
| Profile reference EXPLAIN (§5) | 1,667 ms | 35.068 ms | **48× faster** |
| — buffers | 3,853,170 | 8,063 | **478× fewer** |
| — `smart_content` loops | 962,792 | 1 | **962,792 → 1** |
| `pg_stat_statements` mean, 132 calls | 10,696 ms | 35.068 ms | **~305× faster** |

## The risk I flagged is settled

This was the **unscoped** shape — the profile's reference query, and precisely the worst case I named in round 1 when I warned that the fixture understates prod because `admitted_elements` was unconstrained by the candidate page (484 MB heap / 264,882 live tuples). It came in at **8,063 buffers**, not the 60,000–75,000 I feared, because the fix bounds `admitted_elements` and `smart_map` to the page. **That round-1 concern is resolved by the fix itself.**

The HNSW-driver clause — the one I added because the seeded corpus could not settle it, and which became load-bearing once admission moved back inside the `ORDER BY`/`LIMIT` as a *hash* semi-join — **passes on real prod statistics**. The planner keeps the index driver.

## Correction to my own round-2 record

I reported `smart_content` loops moving 1 → 160 and wrote that the plan's *"resolved ONCE"* prose no longer described the statement. **On prod the loops are 1.** The 150–160 figure is a fixture artefact: all 20,000 seeded vectors share one identical vector and the small tables make a nested loop cheapest, so the fixture exaggerates per-candidate probing that prod does not do. The coder's prose change to "once per page" is still the more precise wording and worth keeping, but my characterisation over-generalised from the fixture. That is now the **third** time the fixture has misled about prod behaviour (the other two: my round-1 buffer-count fear, and the round-1 latency figure).

## Scope limits — read these before citing

1. **One run**, from `psql`, against prod data — **not** through the daemon and **not** post-bounce. It measures the statement and the planner's choice on prod statistics; it does not measure the daemon path, connection pooling, or candidate-expansion passes in the running service.
2. **ac-0005 is unaffected and still owed** — client wall-time, open-job-bracketed, scoped and unscoped interleaved, plus the must-return-results gate. That needs the bounce.
3. The query vector is a **real stored embedding** sampled from `embeddings_1024`, not the byte-identical bind from `dbprof/explain160.out` (which I do not have). So this is the profile's *shape* — `limit=40`, `candidate_limit=160`, unscoped — with a genuine query point, not a byte-exact replay.
4. Two small read-only `SELECT`s were made to obtain binds: one `GROUP BY model_key` count, and one `LIMIT 1` vector fetch. Disclosed for completeness; nothing written.

**Raw plan retained**: `.harness/temp/agent/_prod_explain.out` (171,538 bytes, full ANALYZE/BUFFERS/VERBOSE JSON).

Offer: this is evidence for a receipt o-prime owns. Say the word and I will fold it into the round-2 record as a checked `vd-ac04`, or leave it as this standalone file.
