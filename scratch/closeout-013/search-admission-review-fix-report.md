# search-admission review correction — receipts

Commit: `b332f46e461ac9fd9706f249940502b5740ade67`
PR: https://github.com/AI-Substrate/flowspace3/pull/101

## f-9c41 / f-2e07

- Option B: raw+smart scope source keys pre-resolved once and applied inside HNSW before `ORDER BY/LIMIT`; smart payload and deterministic chooser remain above and page-bounded.
- `admitted_elements` bound is the required union: raw candidate source hashes plus raw hashes reached through smart candidates' `smart_content.text_hash`.
- Paired geometry: 12,000 nearer foreign vectors + five farther scoped vectors, limit 10 → `Ok`, five scoped hits, pass 1. Query elapsed varied with host load; final complete run 74.025 ms (earlier 45.811/46.596 ms). The review target `<40 ms` is not claimed; production ACs remain the performance verdict.
- No-growth geometry: repo-scoped 200 functions queried as docs → zero hits, `candidate_limit_exhausted=true`, pass 2.
- Filtered expansion bound: returns short page metadata; `query_embeddings` keeps its existing error.

## Mutation receipts

- Admitted growth removed: `admitted_growth_stops_an_empty_content_filter_after_two_passes` RED because `candidate_limit_exhausted=false` (`artifact://277`). Restored; GREEN in final 3/3 run.
- Bound Error restored: `filtered_search_returns_a_short_page_at_the_expansion_bound` RED, actual `Error` vs expected `Return { scan_incomplete: true }` (`artifact://279`). Restored; GREEN in embeddings 5/5.
- Shipped JIT setup removed: `shipped_search_transaction_disables_jit_locally` RED, actual `on` vs expected `off` (`artifact://281`). Restored; GREEN in embeddings 5/5.
- Old correlated SQL mutation remains structural/non-ANALYZE and restores correlated smart-content SubPlan + Materialize/elements.

## f-7b13

`configure_search_transaction` is the production path used by `search_elements`. The JIT test forces `jit=on`, invokes that exact helper, then requires transaction-local `off`; removing the shipped statement turns it red as above.

## f-4d88 / NIT

- PR body now states parity honestly: six predicates × two limits. Model, max-distance, ddoc id/gate/schema, worktree, collapse, and provenance are attributed to existing focused regressions, not the golden.
- Stale iterative-scan prose replaced: scope keys are inside HNSW; payload/chooser work is page-bounded; filtered exhaustion is successful metadata.

## Shape and regressions

- Shape: `cargo test -p fs3-store search_plan_shape -- --nocapture` — 2/2. Latest measured fixture: 7.322 ms, 3,113 shared hits, HNSW/candidate rows 160, smart-content max loops 159; no correlated smart-content admission probe; four-field candidate target; no JIT.
- Embeddings unit contracts: 5/5, including JIT and no-Err bound.
- Store review tests: search-admission 3/3; focused store regressions 53/53.
- Daemon focused regressions 56/56, including `scan_incomplete` on Rust `SearchOutcome` and HTTP envelope meta.
- `cargo clippy --all-targets -- -D warnings` — green.
- Every database test ran only on `flowspace3-db-test` at `127.0.0.1:5434`.

## Additive envelope

`meta` retains every existing field and adds:

- `candidate_limit_exhausted: bool`
- `scan_incomplete: bool`
- `passes: number`

`scan_incomplete` survives non-empty semantic pages and lexical fusion independently of `empty_because` and `truncated`. Empty exhausted pages reuse existing `empty_because.reason = "scan_incomplete"`.

CI on exact SHA is the remaining gate.

## Exact-SHA CI

GitHub Actions gate passed on `b332f46e461ac9fd9706f249940502b5740ade67` in 6m03s: run `33598305392`, job `100146227617`. No further pushes pending the detached delta verdict.

## Final diagnosis patch

Commit `8d04a77ead8ea82241fb9fd7d968a1986018ad40`: completed candidate scans no longer raise `scan_incomplete`; the expansion ceiling still does. The envelope publishes only `scan_incomplete` and `passes`; the internal exhaustion name is absent. Plan/implementation prose says smart mapping and representative resolution occur once per page and explicitly bounds every smart-content node's loops by `candidate_limit`.

Targeted unit and HTTP tests plus Clippy passed. Exact-SHA GitHub Actions gate passed in 5m18s: run `33599993525`, job `100151355357`.
