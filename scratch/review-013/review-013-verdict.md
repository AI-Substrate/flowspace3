# review-013 verdict — plan 013-search-admission

**SHA reviewed**: `beee1491be13f3920affc5d257eb580974188360` (branch `013-search-admission`, detached in `/Users/jordanknight/substrate/flowspace/fs3-review-013`)
**PR head**: `065acfd` — `git diff --stat beee1491..065acfd` = 3 files, +13/−4, all under `docs/plans/013-search-admission/assets/tasks/phase-1/`. **Docs-only, confirmed.** Review transfers to the PR head unchanged.
**Record**: `docs/plans/013-search-admission/assets/reviews/cross-model-review.dd.json` — built, and `ddocs validate` → `status: ok`, 0 errors / 0 warnings.

---

## VERDICT: REQUEST CHANGES

One CRITICAL regression, three MINOR, one NIT. Every acceptance criterion that is currently measurable is **TRUE** — the blocking finding is not an unmet criterion, it is a correctness regression the criteria do not cover.

| AC | Judgment | Basis |
|---|---|---|
| ac-0001 | **TRUE** | Reviewer ran `search_plan_shape` 5×, exit 0, 2 passed each. Author's numbers reproduce. |
| ac-0002 | **TRUE** | Golden provenance independently re-derived — not circular. |
| ac-0003 | **TRUE** | Reviewer re-ran: store 53/53, daemon 55/55, both exit 0. |
| ac-0004 | **NOT YET MEASURABLE** | Prod unbounced; load1 never < 15. Thresholds pre-registered. |
| ac-0005 | **NOT YET MEASURABLE** | Same. Protocol + an added pass condition pre-registered. |

## The blocking finding — f-9c41

**Moving admission from inside `candidate_vectors` to a post-filter after the vector `LIMIT` turned a working scoped search into a hard error.**

The old CTE applied the admission `EXISTS` inside its `WHERE`, so `LIMIT $14` bounded the *admitted* page and `candidate_count` counted admitted rows. The rewrite bounds the *raw* page first (`embeddings.rs:505-513`) and admits afterwards (`:584-589`), so `candidate_count` (`:515-517`) now counts raw vectors scanned. The expansion loop's early exit is `hit_count >= filters.limit || scanned < candidate_limit` (`:749`) — and while the corpus exceeds the page, `scanned == candidate_limit` on **every** pass. The exit can never fire. The loop runs all nine passes (40 → 10,240 for `limit=10`) and raises `candidate_limit_exhausted` (`:767-768`).

**Paired probe, one fixture, only the SQL swapped** — 12,000 foreign vectors nearer the query than 5 in-scope vectors, repo-scoped, `limit=10`:

- **Shipped**: `Err(… semantic search could not fill 10 distinct elements after 8 candidate expansions)`
- **Old query** (recovered verbatim from `origin/main`): `Ok with 5 hits`, all `git:github.com/fixtures/scoped`

Independent SQL sweep under the shipped session GUCs across the exact expansion sequence: NEW returns `hits=0, scanned==candidate_limit` at 40/80/160/320/640/1280/2560/5120/10240; OLD returns `hits=5, scanned=5` at every step, i.e. it exits on pass 1.

**And the failure is expensive.** Per-pass `EXPLAIN (ANALYZE, BUFFERS)`, summed: NEW = 9 passes, **199.7 ms, 246,496 shared blocks → then ERROR**. OLD = 1 pass, **29.33 ms, 24,576 shared blocks → correct answer**.

Three things make this production-shaped rather than theoretical:

1. It is the geometry `crates/daemon/tests/search_scope_starvation.rs` was *written to defend* — "a repository holding a small share of a central index can therefore have every candidate deleted by its own anchor and be answered with silence."
2. The old code survived it because the anchor predicates were inside the CTE, where `hnsw.iterative_scan = strict_order` could keep pulling batches. After the move iterative scan has almost nothing left to filter; the whole recovery burden falls on the bounded Rust loop, whose ceiling is `limit × 1024`.
3. It surfaces as an **outage**, not a short answer: the error propagates through `crates/daemon/src/search.rs:341-345` via `map_err(fail)` — contradicting the design that same file states at `:576-580`, *"failing it now over a diagnostic query would turn a working empty result into an outage."*

**Smallest fix**: make the sentinel reflect *admission* exhaustion rather than raw-page fullness — carry `count(*) FROM admitted_candidates` alongside `candidate_count` and stop expanding when the admitted set stops growing. Failing that, return the short page instead of `Err` at the bound (what the old query effectively did). Restoring only the anchor predicates inside `candidate_vectors`, leaving the expensive `smart_content` resolution outside, would also fix it and keep the O(candidates) win.

## The other four

- **f-2e07 (MINOR)** — no existing test can reach f-9c41, and the new regression test does not discriminate: `search_expands_after_a_fully_foreign_first_candidate_page` is **green under the old query too** (I ran it). Its corpus is 40+10, so page 2 already exceeds it. `search_scope_starvation` uses `DECOYS = 1_000` / `TARGET_ELEMENTS = 200`, ~10× below the 10,240 ceiling.
- **f-7b13 (MINOR)** — the plan-shape JIT assertion cannot fail: `bounded_plans` itself executes `SET LOCAL jit = off` (`:1236-1238`) before the EXPLAIN the assertion reads (`:1307-1310`). ac-0001 still passes on its stated alternative limb.
- **f-4d88 (MINOR)** — parity covers 6 predicates × 2 limits, not the 12 contracts the PR body's table claims. `model_key`, `max_distance`, `id_kinds`, `gate_open` (incl. the unknown-state limb), `ddoc_schema`, `worktree` are exercised by no parity case. MINOR because I diffed those predicates old-vs-new and they are textually identical.
- **f-6a55 (NIT)** — `embeddings.rs:669-685` still documents filters as living inside the CTE and credits `ITERATIVE_SCAN` with the selective-anchor rescue. That defence is exactly what this change removed.

## Confirmed-good (zero findings spent, hunted and cleared)

- **Golden provenance is real** — I replaced `SEARCH_ELEMENTS_SQL` with the recovered old statement and `search_parity_matches_old_query_golden` **passed**. ac-0002 is not circular. This was my primary intake suspicion.
- **Chooser semantics preserved exactly** — old `ORDER BY el.id LIMIT 1` / `ORDER BY sc.created_at, sc.model_key, sc.raw_hash LIMIT 1` are reproduced key-for-key by `admitted_representatives` / `smart_representatives`. The owed-1 "existential multiplicity" risk is genuinely defended; no caller-visible row is lost.
- **JIT does not leak** — `SET LOCAL jit = off` verified empirically: `on` → `off` in-transaction → `on` after COMMIT, same session.
- **Fixture widening is a real fix, not a bigger coin** — 5/5 stable, no shape variance.
- **The mutation is honest** — it splices valid SQL and asserts both the correlated SubPlan and the Materialize-over-elements node; it is not a parse failure scored as a pass.

## Pre-registered for o-prime's post-merge receipt

**ac-0004** — inside `BEGIN READ ONLY` with `SET LOCAL statement_timeout='30s'`, `SET LOCAL max_parallel_workers_per_gather=0`, at load1 < 15, no retention purge running. PASS needs all four: smart_content `Actual Loops` < 1,000; shared hits < 100,000; `Execution Time` < 300 ms; **and** `embeddings_1024_vector_idx` still the childless `<=>`-ordered driver with `Actual Rows ≤ 160` (my addition — the seeded corpus cannot prove the planner keeps that shape on prod statistics, packet owed-1(c)).

⚠️ The fixture understates one prod cost: `admitted_elements` is unconstrained by the candidate page, so an unscoped prod search materializes the whole eligible elements set — **484 MB heap / 264,882 live tuples** per the profile report — against a fixture whose 50,000 elements carry `'shape body N'` as `raw_text`. **The 2,691-hit receipt is not a safe predictor of the prod buffer count.**

**ac-0005** — `uptime` + `flowspace3 status` open-job count immediately before *and* after every timing; discard runs bracketed by ≳50 open jobs; interleave scoped and unscoped. PASS needs: 3× main-checkout query < 1 s; both 60 s-timeout queries < 2 s; unscoped doc-heavy from `/Users/jordanknight/substrate/chainglass` < 5 s. **Added by me, and it should gate the merge ahead of the timing: every run must RETURN RESULTS, not `candidate_limit_exhausted`. A scoped run that errors is an ac-0005 failure however fast it fails.** Baseline pair: your 10,696 ms mean over 132 calls.

## Fences and hygiene

- Tests ran **only** on `flowspace3-db-test` `127.0.0.1:5434`. Prod `:5433` was never touched; `:7373` only as an ordinary `search`/`status` client. **No prod EXPLAIN was taken** — load1 was 21.74 / 23.82 / 38.87 throughout, never below the authorised ceiling of 15.
- No `harness checks`; targeted `cargo test` only. CI read, not rerun.
- All reviewer code edits were uncommitted and reverted: `git status` shows `crates/` clean at the reviewed sha. The only modified tracked files are the two `packet-reviewer.dd.*` files, which are **your** rewrite, not mine.
- Scratch database `fs3_review013_hunt1` on `:5434` dropped.
- The 768-char packet truncation I reported at ack is **retracted** — `awk '{print length}'` shows the longest line whole at 1,833 chars. Your call was right; it was my viewer.
