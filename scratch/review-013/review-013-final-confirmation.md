# review-013 final confirmation — sha 8d04a77

**CONFIRMED — both fixes present, correct, and discriminating. Nothing else changed under `crates/`. Clear to merge and bounce.**

`git diff --name-only b332f46..8d04a77` → 4 files under `crates/` (+12/−12), all confined to f-3b7e and f-5c92, plus docs/plan records.

- **f-3b7e FIXED** — `expansion_decision` now ANDs `!candidate_scan_exhausted` (`embeddings.rs:100-103`), and my exact case is committed as a unit assertion: `expansion_decision(1, 10, 100, 160, 5, Some(5), 3)` → `Return { scan_incomplete: false }`. **It discriminates**: reverting only the new conjunct turns it red (`left: Return { scan_incomplete: true }`, `right: … false`), so the test defends the fix rather than describing it. Source restored.
- **f-5c92 FIXED** — `candidate_limit_exhausted` removed from `SearchOutcome` and from the HTTP envelope; one published key (`scan_incomplete`). The daemon test now asserts the removed key is **absent** (`envelope["meta"].get("candidate_limit_exhausted").is_none()`), which is the right shape — it guards the removal instead of merely not checking it.
- **Plan prose** — goals and the ac-0001 claim now read "resolved once per page, never as a correlated SubPlan" with "smart_content `Actual Loops` ≤ `candidate_limit`". The criterion's words now match the number it passes on, which was the point of my round-2 note.

**Green at 8d04a77** (`:5434`, targeted, exit 0): expansion unit 1/1 · plan-shape 2/2 (`execution_ms=7.1`, `shared_hits=3103`, `hnsw_rows=160`, `candidate_rows=160`) · search_admission 3/3 · search_scope_starvation 9/9.

**Two accuracy corrections to my round-2 record, neither actionable:**

1. I wrote that loops sit "exactly at the ceiling with zero margin". This run measured `smart_content_max_loops=150` against a bound of 160 — so the value tracks how many of the page's candidates are smart, and there is margin. The bound is real, not coincidental; my "zero margin" phrasing was overstated.
2. The assertion is `shipped_loops <= 160.0` with `160` also the literal bind in `explain_search`. Self-consistent and correct today, but the two literals are independent — a future change to the bind must remember the assert. Not worth a finding; noting it so it is on the record rather than rediscovered.

Carried to follow-up as ruled: **f-7b13**, and **f-4d88** with its shifted risk — the parity gap that now matters is `worktree` plus the conversation/scope legs (`scope_blobs`/`scope_representatives`), since those are the newly-written code below the vector `LIMIT`.

Hygiene: `:5434` only, prod untouched, no `harness checks`, all reviewer edits reverted (`git diff` on `crates/` = 0 lines).

**ac-0004/ac-0005 receipts remain owed post-bounce**, thresholds unchanged from round 1, with the HNSW-driver clause now the load-bearing one: admission is back inside the `ORDER BY`/`LIMIT` as a **hash** semi-join, a different cost object from the old correlated `EXISTS`, and only prod statistics settle whether the planner keeps the index driver.
