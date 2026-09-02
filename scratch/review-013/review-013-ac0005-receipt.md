# review-013 — ac-0005 receipt (post-bounce, prod 013 binary)

**Taken**: 2026-09-02 ~17:00 local, against prod daemon **pid 74144**, `/Users/jordanknight/substrate/flowspace/flowspace3/target/release/flowspace3 daemon`, started 16:57:45, bound to `127.0.0.1:7373`.
**Mode**: ordinary read-only client. No EXPLAIN, no writes, no daemon control.

## Binary identity verified functionally, not assumed

The running daemon's search envelope carries `"scan_incomplete": false` and `"passes": 1`, and **does not** carry `candidate_limit_exhausted`. That is the exact signature of the reviewed code: the `scan_incomplete` carrier added for f-9c41's fix, with the f-5c92 single-key change applied. The measurement therefore provably ran against `8d04a77` / `c2f4709`, not an older build.

## Results — 8 runs, scoped and unscoped interleaved

| run | cwd | wall | rc | open jobs + load1 before → after | gate |
|---|---|---|---|---|---|
| R1 scoped ref #1 | flowspace3 | **0.55 s** | 0 | 0 / 20.27 → 0 / 20.27 | PASS — 10 results, `scan_incomplete=false`, `passes=1` |
| R2 unscoped doc-heavy | chainglass | **0.70 s** | 0 | 0 / 20.27 → 0 / 20.27 | PASS — 10 results |
| R3 scoped ref #2 | flowspace3 | **0.75 s** | 0 | 0 / 20.27 → 0 / 20.27 | PASS — 10 results |
| R4 prior-slow (jobs table) | flowspace3 | **0.69 s** | 0 | 0 / 20.27 → 0 / 18.97 | PASS — 10 results |
| R5 unscoped doc-heavy | chainglass | **0.74 s** | 0 | 0 / 18.97 → 0 / 18.97 | PASS — 10 results |
| R6 scoped ref #3 | flowspace3 | **0.61 s** | 0 | 0 / 18.97 → 1 / 18.97 | PASS — 10 results |
| R7 prior-slow (watcher) | flowspace3 | **0.54 s** | 0 | 2 / 18.97 → 2 / 18.97 | PASS — 10 results |
| R8 unscoped doc-heavy | chainglass | **0.45 s** | 0 | 2 / 17.93 → 2 / 17.93 | PASS — 10 results |

Every run: `ok=true`, 10 results, `scan_incomplete=false`, `passes=1`, `empty_because=null`.

## Scored against the pre-registered thresholds

| Criterion | Threshold | Measured | Verdict |
|---|---|---|---|
| Main-checkout reference query, 3 runs | all < 1 s | 0.55 / 0.75 / 0.61 s | **PASS** |
| Previously-slow queries | < 2 s each | 0.69 s, 0.54 s | **PASS** |
| Unscoped doc-heavy from `chainglass` | < 5 s | 0.70 / 0.74 / 0.45 s | **PASS** |
| **Must-return-results gate** *(reviewer addition)* | every run returns results, no `candidate_limit_exhausted`, no silent under-fill | 8/8 returned 10 results, `scan_incomplete=false` | **PASS** |
| Queue control | report only runs under ~50 open jobs | 0–2 open jobs throughout | all 8 admissible |

**`passes=1` on every run** is the strongest single signal: the candidate page is satisfied on the first attempt, so the expansion loop never engages. That is the mechanism of f-9c41 working as designed, observed in production rather than inferred.

## Against the before-numbers

| Baseline | Before | After | Change |
|---|---|---|---|
| `pg_stat_statements` mean, 132 calls (o-prime) | 10,696 ms | 450–750 ms | **~14–24×** |
| Observed single search at load ~30 (o-prime) | 12.5 s | ≤ 0.75 s | **~17×** |
| Profile report's own dogfooded search (§ closing) | 15.6 s | ≤ 0.75 s | **~21–35×** |

## Honest caveats

1. **Load was hostile, not quiet.** load1 ran 17.93–20.27 across the window. The plan's controlled variable is the open-job count (0–2, far under ~50), which is satisfied, but nobody should read these as best-case numbers — they were taken on a loaded box and are therefore **pessimistic**. A quiet box would be faster.
2. **The two 60 s-timeout coder queries are not quoted verbatim anywhere in `assets/inputs`.** I substituted the two slow queries that *are* documented: `"how does the daemon poll the jobs table for work"` (recorded at 15.6 s in the profile report's closing section) and `"how does the watcher decide what to ignore"` (the `search_scope_starvation` question). If o-prime holds the literal timeout strings, re-running those two is a five-second job and I will do it on request.
3. **Fresh daemon.** pid 74144 had been up ~2 minutes when R1 ran. R1 was the first query it served and came in at 0.55 s, so no cold-start penalty is hiding in the later runs.
4. These are wall-clock client timings including CLI startup and JSON serialisation, not server-side execution time. The server-side figure is the ac-0004 receipt's 35.068 ms.

**Harness retained**: `.harness/temp/agent/_ac0005.sh` — re-runnable, records the bracketing itself.


---

## Second pass — quiet window, taken at o-prime's invitation

Re-run of the identical eight-run protocol at **load1 12.92–12.93** (the first pass was 17.93–20.27), open jobs **0 throughout**.

| run | cwd | wall | gate |
|---|---|---|---|
| R1 scoped ref #1 | flowspace3 | 0.53 s | PASS |
| R2 unscoped doc-heavy | chainglass | 0.77 s | PASS |
| R3 scoped ref #2 | flowspace3 | 0.60 s | PASS |
| R4 prior-slow (jobs) | flowspace3 | 0.49 s | PASS |
| R5 unscoped doc-heavy | chainglass | 0.57 s | PASS |
| R6 scoped ref #3 | flowspace3 | 0.50 s | PASS |
| R7 prior-slow (watcher) | flowspace3 | 0.56 s | PASS |
| R8 unscoped doc-heavy | chainglass | 0.62 s | PASS |

All eight: `ok=true`, 10 results, `scan_incomplete=false`, `passes=1`, `empty_because=null`.

### The load sensitivity is gone — and that is the headline

| window | load1 | min | max | mean |
|---|---|---|---|---|
| hostile | 17.93–20.27 | 0.45 s | 0.75 s | **0.629 s** |
| quiet | 12.92–12.93 | 0.49 s | 0.77 s | **0.580 s** |

A 0.049 s difference in means — **7.8%**, inside run-to-run noise. Sixteen runs across two load regimes are all sub-second, and the ranges overlap almost completely.

This matters more than either number alone. ac-0005's protocol exists *because* search used to be violently load-sensitive: the plan's own text cites peer evidence of the same query moving from **156 s to 6 s** as queue load changed, which is why queue state was made a controlled variable in the first place. That coupling is no longer observable. The controlled variable turned out not to need controlling — which is the strongest available evidence that the fix removed the quadratic term rather than merely making it smaller.

Caveat: both windows were sampled on a busy workstation (load1 never below 12.9) with an idle job queue (0–2). This shows insensitivity *across the sampled range*, not that no coupling could ever appear at extremes.
