# review-013 delta verdict — plan 013-search-admission, round 2

**Fix sha**: `b332f46e461ac9fd9706f249940502b5740ade67` (round-1 sha was `beee1491`)
**Record**: `docs/plans/013-search-admission/assets/reviews/cross-model-review-round2.dd.json` — built, `ddocs validate` → ok, 0 errors / 0 warnings.
**CI on the sha**: `gate pass`, 6m3s. Read, not rerun.

---

## VERDICT: APPROVE

The blocker is fixed and verified in both directions. Two new non-blocking items, two carried MINORs correctly outside the ruling's scope.

## f-9c41 — FIXED

Same 12,000-nearer-foreign + 5-in-scope geometry, production session GUCs, full expansion sweep:

| candidate_limit | Round 1 (`beee1491`) | Fix (`b332f46`) | Pre-013 |
|---|---|---|---|
| 40 → 10,240 | `hits=0, scanned==limit` every step → **Err** | **`hits=5, scanned=5, admitted=5`** every step | `hits=5, scanned=5` |

The fixed statement is indistinguishable from the pre-013 statement at every step. Through the real Rust, `scoped_search_passes_twelve_thousand_nearer_foreign_vectors` passes with `passes ≤ 2`, `candidate_limit_exhausted = false`, all five identities scoped.

## f-2e07 — FIXED, and the test genuinely discriminates

I spliced the pre-fix SQL back in with a `0::bigint AS admitted_count` shim so failure would be **behavioural, not schema**. It went red:

```
paired_geometry: passes=2 hits=0
panicked: all scoped vectors must be returned:
  SearchPage { hits: [], passes: 2, candidate_limit_exhausted: true }
  left: 0   right: 5
```

That shim incidentally proved **the two halves of the fix are independently load-bearing**: the Rust sentinel alone already converts round 1's nine-pass outage into a two-pass diagnosed empty page; recovering the five hits requires the SQL relocation.

## f-6a55 — FIXED. All four binding notes implemented as written

`admitted_sources` below the `LIMIT`; **both** key legs inside it (raw `:622-623`, smart `:624-625`); a distinct carrier on all three `SearchOutcome` exits plus the HTTP envelope with `truncated` correctly left alone; no-`Err` scoped to `search_elements` only — the sole surviving `Err(candidate_limit_exhausted)` is `:413` inside `query_embeddings`.

**Bonus**: bounding `admitted_elements` and `smart_map` to the page took the plan-shape ANALYZE from **105.5–121.3 ms → 6.873–7.315 ms**, 5/5 deterministic.

## ACs at the fix sha

| AC | Judgment | Basis |
|---|---|---|
| ac-0001 | **TRUE** | 5/5 deterministic; see the loops note below |
| ac-0002 | **TRUE** | Golden byte-identical across shas, so round 1's provenance control carries |
| ac-0003 | **TRUE** | Store 53/53, daemon **56/56** (one more than round 1) |
| ac-0004 / ac-0005 | **Deferred** | Prod still unbounced; round-1 thresholds stand |

## Two new, non-blocking

**f-3b7e (MINOR, proven not reasoned)** — `expansion_decision` can label a *completed* scan as `scan_incomplete`. When a pass both exhausts the candidate scan (`scanned < candidate_limit`) **and** sees `admitted` stall on that same pass, both flags are true and the short page is marked incomplete although nothing further exists. Observed via a disposable unit test:

`expansion_decision(1, 10, 100, 160, 5, Some(5), 3)` → `Return { scan_incomplete: true }`

It matters because `search.rs:502-508` now branches on that label to decide whether to suppress the filtered-empty steer — so a legitimately empty content-filtered search can be reported as a bounded short scan. That is the exact inverse of the confident-lie failure `search.rs:405-410` exists to prevent. **One-line fix**: `&& !candidate_scan_exhausted`. The existing unit test at `:1037` covers only the ceiling case with a growing admitted set, so it cannot see this. Recommend taking it now — a diagnosis bug in brand-new code.

**f-5c92 (NIT)** — `search.rs:350-352` does `let scan_incomplete = candidate_limit_exhausted;` and `http.rs` publishes both, so the envelope carries two agent-facing keys that are aliases by construction. Publish one, or give them the genuinely different meanings f-3b7e's fix would create (*ceiling reached* vs *answer is short*).

## Carried, unchanged, correctly outside the ruling

- **f-7b13** — JIT assertion still unfalsifiable: `bounded_plans` now calls `configure_search_transaction`, which disables JIT before the asserted EXPLAIN. The refactor is *more* faithful to production; the assertion still cannot fail.
- **f-4d88** — but its **risk profile shifted**. At round 1 every unexercised predicate was textually identical to its predecessor. Now `worktree` and the conversation/scope legs are **new code below the LIMIT** (`scope_blobs`/`scope_representatives`, `:579-604`) with no parity case. Worth a follow-up parity case for those two specifically.

## Record rather than fix

`smart_content` max loops went **1 → 160** — probed once per candidate rather than resolved once. That is exactly `candidate_limit`, so ac-0001 passes on its numeric bound with **zero margin**. It is not a correlated SubPlan (the test still asserts none, and passes), and it bought the 15× latency win. But the plan's goal prose — *"smart_content text_hash→raw_hash resolved ONCE"* — no longer literally describes the statement. The criterion now passes on its number, not its words.

## One note for ac-0004

Your adopted HNSW-driver clause is **more** load-bearing than it was: the admission semi-join moved back inside the `ORDER BY`/`LIMIT` as a **hash** semi-join — a different cost object from the old correlated `EXISTS`. Only prod statistics can settle whether the planner keeps the index driver.

## Hygiene

`:5434` only; prod never touched; no `harness checks`; CI read not rerun; all reviewer edits uncommitted and reverted (`git diff` on `crates/` = 0 lines); scratch database dropped.
