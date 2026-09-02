# review-013 ack — cross-model reviewer, plan 013-search-admission

**Reviewer**: Claude (github-copilot/claude-opus-5), cross-model per impl-guide `review.model_override`.
**SHA under review**: `beee1491be13f3920affc5d257eb580974188360`, branch `013-search-admission`, detached in `/Users/jordanknight/substrate/flowspace/fs3-review-013`.
**Counterpart**: o-prime (`pij-binding-magpie`). I will never `pij send pij-instant-lynx`, never `pij adopt`.
**Status**: ACK ONLY. No review steps executed yet. Awaiting o-prime ruling on §5 before running.

---

## 0. Packet intake — what I read, and one intake defect

Read in full: `packet-reviewer.dd.json` (the `.dd.md` render truncates every cell at 768 chars, so the owed lists are only complete in the JSON — I extracted them from source), `plan.dd.json` (ac-0001..0005 untruncated), `impl-guide.dd.json` (units, risks 1–4), `assets/inputs/db-cpu-profile-report.md`, `.harness/temp/agent/search-admission-pr-body.md`, `.harness/temp/agent/search-admission-gate-failure.md`, the shipped `SEARCH_ELEMENTS_SQL` (`crates/store/src/embeddings.rs:505-656`) and the pre-change SQL recovered from `origin/main` for a line-by-line semantic diff.

**Intake note for o-prime (not a finding, no findings budget spent):** for roughly the first minute of this seat the rendered `packet-reviewer.dd.md` was the unfilled template (`<plan slug>`, `<ord>-<slug>`, no sha, no owed lists) while `packet-reviewer.dd.json` already carried the real brief. Per i1b I was about to refuse and ask; your interjection landed first. The durable hazard is that **`.dd.md` truncates long cells at 768 chars** — i6, owed-1 and owed-2 are all cut mid-sentence in the render. A reviewer who obeys i1's "read the rendered .dd.md" gets a silently amputated brief. Encoding: reviewers should read the `.dd.json`, or `ddocs build` should not truncate instruction cells.

**i-head, discharged now:** `git diff --stat beee1491..065acfd` = 3 files, +13/-4, all under `docs/plans/013-search-admission/assets/tasks/phase-1/` (`execution.log.md`, `tasks.dd.json`, `tasks.dd.md`). **Docs-only. Confirmed. No code.**

## 1. What the diff actually is (my own read, not the author's prose)

The rewrite is not "the same query with joins". It **moves admission from inside the vector CTE to after it**:

- **Old** (`origin/main`): `candidate_vectors` = the `$14` nearest vectors **that already passed the full admission `EXISTS`** — `ORDER BY vector <=> $1 LIMIT $14` applied to *admitted* rows. `hnsw.iterative_scan = strict_order` was the mechanism that kept pulling batches when a selective filter ate them (the "19 of 120" doc comment).
- **New** (`:505-513`): `candidate_vectors` = the `$14` nearest vectors filtered only by `model_key`, `source_kind`, `max_distance`. Admission is a post-filter join at `:584-589`. Selectivity recovery has therefore **moved out of pgvector's iterative scan and into the Rust expansion loop** (`:717-771`), and `candidate_count` changed meaning: old = admitted rows found, new = raw vectors scanned.

Two things I checked and am **provisionally satisfied by**, so I will not spend hunt budget re-deriving them from scratch (I will confirm by test, not by re-reading):
- The chooser semantics are equivalent. Old smart chooser: `ORDER BY sc.created_at, sc.model_key, sc.raw_hash LIMIT 1` gated on an eligible element. New `smart_representatives` (`:572-575`): `DISTINCT ON (text_hash) ORDER BY text_hash, created_at, model_key, raw_hash` over `smart_map`, which is gated on `admitted_representatives`. Same key, same order.
- Old element chooser: `ORDER BY el.id LIMIT 1` over elements passing every filter. New `admitted_representatives` (`:559-562`): `DISTINCT ON (raw_hash) ORDER BY raw_hash, id` over `admitted_elements`. Same.
- So **owed-1(b) "existential multiplicity"/result-loss is NOT where I expect the defect** — the old query already collapsed to one representative per raw hash. I will still prove it (step 2.4) because it is cheap, but I am reallocating hunt weight to §2.1 and §2.2 below.

## 2. Numbered plan — how each AC gets proved or refuted

All database work on `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test` (container `flowspace3-db-test`, confirmed `Up (healthy)`). Never `:5433`, never `:7373`. Targeted `cargo test` only — I will not take the exclusive `harness checks` gate slot. I will not rerun CI; I will read PR #101's run.

### ac-0001 — loops bound, mutation-checked

1. **Re-run, read exit code**: `cargo test -p fs3-store search_plan_shape -- --nocapture`. Record pass/fail from `$?`, not from output prose.
2. **Re-derive the author's numbers myself**: capture the shipped `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` emitted by the test and independently assert from the JSON: max `smart_content` loops ≤ `candidate_limit`; no `Materialize` over `Seq Scan on elements`; `candidate_vectors` output list is exactly `(source_hash, source_kind, chunk_no, distance)` with no `vector`; no `JIT` node. Compare each against the PR body's claims (108.468 ms, 2,691 shared hits, HNSW rows 160, max loops 1). A number that does not reproduce is a finding.
3. **Perform the author's mutation myself** — swap `ADMISSION_JOIN_SQL` (`:501-503`) for `fixtures/search_admission_old.sql` and confirm the test goes RED and that the mutant plan really does contain the correlated `smart_content` SubPlan + `Materialize`. I will check specifically that the mutant is **valid SQL that plans**, not a syntax error the test scores as "failed" for the wrong reason — a mutation that fails on parse proves nothing about shape. This is the one place I will touch code, in-memory/scratch only, reverted, never committed (fence: read-only on code — I will do it as an uncommitted local edit and `git checkout --` after, and I will say so in the verdict).
4. **My own extra mutation (owed-2)**: delete the `SET LOCAL jit = off` (`:54`, executed `:706`) and confirm the measured JIT cost *returns* — i.e. that the JIT guard is load-bearing and not decoration on a query whose cost estimate is now sane. If JIT does not return, the plan's stated justification ("cost estimate sane, **or** explicitly disabled with a reason") is still satisfiable but the reason is weaker; I will report which of the two limbs is actually true.
5. **owed-1(e) — is the fixture widening a fix or a bigger coin?** Run `search_plan_shape` **5 consecutive times** and report the pass count. State explicitly whether 20k embeddings makes HNSW-over-seq-sort deterministic or merely probable, and name the cost model margin between the two plans at that size.

### ac-0002 — ranking parity (golden provenance is the risk)

6. **Attack provenance first (i6 1(e))**: `search_parity_matches_old_query_golden` compares against a **committed** `fixtures/search_admission_golden.json`. Nothing in the test re-derives it from the old query, so as committed the golden is an assertion, not evidence — if it was captured from the *new* code the test is circular and ac-0002 proves nothing.
   **Re-derivation**: I will seed the identical fixture, then execute the **real pre-change SQL** (recovered verbatim from `origin/main:crates/store/src/embeddings.rs`, not the admission fragment) against it via `psql`/sqlx with the same 14 binds, and diff its top-N addresses+scores against the committed golden at 1e-6. Golden confirmed ⇒ ac-0002 stands on evidence. Golden differs ⇒ finding, with the differing rows named.
7. **Filter matrix, held to the author's own list (i6 1(b))**: for each of `model_key`, `source`, `max_distance`, `kinds`, `id_kinds`, `gate_open` **including the unknown/NULL-state limb** (`:525-535`), `ddoc_schema`, `conversation` prefix, `repo`, `path` glob, `worktree` — assert the old and new queries return identical top-N on the same fixture. `gate_open` unknown-state and `path` glob against `c.worktree LIKE $7` on the conversation leg are the two I expect to be weakest.
8. **owed-1(b)/(c) multiplicity + provenance**: fixture already claims a raw hash shared by ≥3 elements and a summary shared by ≥2 raw bodies. I will verify those rows are actually *reachable in the top-N* (a shared-hash row that never ranks proves nothing) and that the chosen representative is **stable across 3 runs** and identical to the old query's choice. Plus: diff the **full JSON envelope** (not just addresses) for the same query old vs new, so `identity`/`root_path`/`path` provenance drift is caught.

### ac-0003 — existing suites unchanged

9. Re-run, exit codes read, each separately: `cargo test -p fs3-store --test pg_first_light --test pg_ddoc --test pg_store_flows`; daemon `conversation_query`, `first_light`, `oversize`, `search_empty`, `search_lexical`, `search_scope_starvation`. I will confirm the counts the author claims (53/53, 55/55) rather than accept them. `search_scope_starvation` is the one I care about — see §2.1.

### ac-0004 — prod EXPLAIN — see §5, blocked on ruling

10. I am forbidden prod. **Surrogate I will run instead on `:5434`**: seed to prod's measured shape (86,191 elements per the profile report §5) and run the shipped statement with the profile's `limit=40 / candidate_limit=160` binds under `statement_timeout 30s`, `max_parallel_workers_per_gather 0`, reporting loops / shared hits / execution time. I will label this **surrogate, not ac-0004** — it cannot discharge a criterion whose text says PROD. I will hand o-prime the exact read-only command to run for the real receipt.

### ac-0005 — latency, load-stated, unscoped doc-heavy — see §5

11. **Load is a controlled variable and it is currently hostile**: at ack time `uptime` reports load average **21.74 / 24.03 / 28.98** on this workstation. Any timing I take now is not evidence. My protocol, once ruled and once load permits:
    a. Record `flowspace3 status` open-job count **immediately before and after every single timing**, and discard any run whose bracketing counts are ≥ ~50 open jobs (plan's own threshold). Record `uptime` alongside each.
    b. `/usr/bin/time -p flowspace3 search 'where does the daemon detect new git worktrees' --json` from the main checkout — **3 runs**, target < 1 s each.
    c. The **two 60 s-timeout queries** from `assets/inputs` (i7) — target < 2 s each.
    d. **UNSCOPED, doc-heavy**: same query shape run from `/Users/jordanknight/substrate/chainglass` — target < 5 s. This is the measurement that matters most (see §2.2) and the one I will refuse to let a narrow scoped win stand in for.
    e. **Interleave** scoped and unscoped runs so a warming pool cannot flatter one shape.
    f. If load cannot be controlled, I will **report both numbers and state the load, and return no verdict on ac-0005** rather than launder a hostile-load timing into a pass.

---

## 2.1 HUNT #1 (highest suspicion) — selective scope now refuses where it used to answer

The post-filter move changes what `scanned < candidate_limit` means. Old: page full of *admitted* rows, so a selective scope exhausted the index and the loop returned a **short but valid** page. New: the page fills with foreign rows, `scanned == candidate_limit` always while the corpus exceeds the page, so the loop **expands instead of returning** — 40 → 80 → … → 10,240 at `limit=10` (`INITIAL_CANDIDATE_MULTIPLIER=4`, `CANDIDATE_GROWTH_FACTOR=2`, `MAX_CANDIDATE_EXPANSIONS=8`) and then raises `candidate_limit_exhausted` at `:767-768`.

**Prediction to test:** seed > 10,240 vectors of which fewer than `limit` are eligible under a repo/path scope. Old query returns the few hits it has; new query **errors**. That is a user-visible regression in exactly the prod shape this plan is about — a small repo inside a large shared index.
**Experiment:** build that fixture on `:5434`, run `search_elements` at `limit=10`, record the outcome; run the recovered old SQL on the same fixture and compare. Also check what `search_scope_starvation` actually asserts and whether its corpus is large enough to reach the bound (if it is not, the green is uninformative and that is a test-gap finding under i3(5)).

## 2.2 HUNT #2 — the unscoped doc-heavy cost, and per-pass O(elements)

`admitted_elements` (`:519-557`) is `MATERIALIZED` and **unconstrained by the candidate page** — it materializes every eligible element in the database, then `admitted_representatives` sorts them, then `smart_map` joins all of `smart_content` against that. For an **unscoped** search that is the whole elements table (86,191 on prod) plus the whole smart_content table, sorted, **once per expansion pass** — and §2.1 says selective/doc-heavy queries now take *more* passes. The plan's goal says "elements admitted ONCE"; the shipped code does it once *per pass*, up to 9 times.

This is why ac-0005's unscoped doc-heavy leg is load-bearing: the 1M correlated probes are certainly gone, but they may be partly replaced by `passes × O(elements + smart_content)`. **Measurement:** instrument the pass count for the reference query before/after (i6 1(d)) by counting statement executions, and time unscoped-vs-scoped separately. A big scoped win with an unscoped regression is a finding, not a pass.

## 2.3 HUNT #3 — the retained doc comment now misdescribes the shipped query

`embeddings.rs:669-685` (kept verbatim from the old code) still explains that the filters live *inside* the CTE and that `hnsw.iterative_scan` is what rescues a selective anchor ("19 of 120 … then 120 of 120"). After this change the filters are **not** inside the CTE, and iterative scan has almost nothing left to recover — the recovery is the Rust loop. I will confirm by reading the shipped SQL against the comment and, if it stands, report it: a comment that documents a mechanism the code no longer uses is how the next agent reintroduces the bug. Low severity, high durability.

## 2.4 Cheaper confirmations (owed-1 (b)(c)(d))

- **JIT scope leak**: `DISABLE_SEARCH_JIT = "SET LOCAL jit = off"` (`:54`) executed on the transaction (`:706`) — `SET LOCAL` is transaction-scoped by construction, so I expect this clean. Proof: `SHOW jit` on the same pooled connection after `search_elements` returns, asserting `on`.
- **Zero-row sentinel**: `candidate_meta LEFT JOIN final_hits ON TRUE` (`:654-655`) returns one all-NULL row when nothing is admitted; Rust filters on `element_id IS NULL` (`:744-765`). I will assert an empty scoped search returns `[]` and not one phantom hit, and that expansion **terminates** rather than looping to the bound on a legitimately-empty index.
- **Scoping byte-identity (owed-1(c))**: diff the full JSON envelopes old-vs-new for path-scope (#91), `include_hidden` default (row 125), worktree fan-out (row 135).

## 3. Dogfooding

I will use `flowspace3 search` (not grep) for every meaning-shaped lookup during the review — consumers of `search_elements`, who reads `candidate_count`, where scope-starvation is tested. Every miss, bad `next_action`, or surprising envelope gets `harness observe` **plus** a pij line to you in-context. I will **capture only** — I will not drain or clear the shared observation buffer (worker rule, DL 2026-08-26).

## 4. Known-open — zero findings, acknowledged

Rows 139/140/141/124b/147 (TypeScript-has-no-symbols / doc-heavy ranking is 147, not this plan); row 131 harness-checks tail truncation; row 145 source-scoping vocabulary; row 155 `progress_is_reported_while_the_queue_is_still_draining` streaming flake; row 150 `.serena` litter; the exclusive gate slot is not mine; CI is read, not rerun. I will not spend a finding on any of these.

## 5. ONE RULING NEEDED BEFORE I START — prod access is contradictory in my brief

The packet tells me both things:

- **i6(2)** — "after o-prime's bounce, run the prod EXPLAIN (ANALYZE, BUFFERS) **yourself** read-only … and the wall-time runs with `flowspace3 status` open-job count logged"; **w1** — "prod :7373/:5433 read-only, EXPLAIN ANALYZE of the SHIPPED query only, under statement_timeout 30s and max_parallel_workers_per_gather 0".
- **owed-2** — "**NEVER touch prod :5433/:7373**; read-only prod search is o-prime's receipt"; and my dispatch instruction says the same: "NEVER touch prod :5433 or the daemon on :7373".

ac-0004 and ac-0005 are *only* provable on prod, and ac-0005 requires running `flowspace3 search`, which is a client of the daemon on `:7373`. So as briefed, the two ACs the plan leaves `unchecked` are the two I am forbidden to check.

**My default if you do not rule** (most conservative, and what I will do): treat prod as untouchable. I will judge ac-0001/0002/0003 on my own fresh evidence, run the `:5434` surrogates in steps 10 and §2.1/§2.2, and return ac-0004/ac-0005 as **NOT PROVEN BY REVIEWER — o-prime's receipt owed**, with the exact command list and the pass/fail thresholds pre-registered so your numbers can be scored without re-litigation.

**What I would prefer**, and what I think the plan needs: authorise me for **read-only prod only** — `EXPLAIN (ANALYZE, BUFFERS)` of the shipped statement under `statement_timeout=30s` / `max_parallel_workers_per_gather=0` on `:5433`, plus `flowspace3 search` and `flowspace3 status` as an ordinary client against `:7373`. I would write nothing, bounce nothing, and never execute the old query on a prod-sized corpus. Independent measurement is the whole point of a cross-model reviewer; if the only prod numbers come from the party being reviewed, ac-0004/0005 are audited prose, which owed-2 explicitly tells me not to accept.

Also please confirm: **has the bounce happened?** Both ACs say "after o-prime's bounce". If prod is still running the old binary, neither is measurable by anyone yet and the honest verdict for both is *not yet measurable*, not *fail*.

Note on timing regardless of the ruling: load average is **21.74 / 24.03 / 28.98** right now. Under the plan's own ~50-open-job rule I would discard timings taken in this state, so ac-0005 may need a quiet window whoever runs it.

---

## 6. Deliverables I owe you

1. This ack — `.harness/temp/agent/review-013-ack.md` (delivered).
2. Verdict — `.harness/temp/agent/review-013-verdict.md`.
3. Review record ddoc — `docs/plans/013-search-admission/assets/reviews/`, **built and `ddocs validate`d** (global `ddocs`, from worktree root) before I call it done; severities MAJOR/MINOR/NIT/NA, kinds defect/dim0/question, ids `<prefix>-<4 hex>`.
4. Every AC judged true/false/not-proven with cited evidence (d1); seams reported (d2); `pij send pij-binding-magpie` with path + one-line verdict (d3).

**Waiting on your §5 ruling before I run anything.**
