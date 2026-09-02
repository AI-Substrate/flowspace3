# search-admission STOP-and-ask 008 — existing scoped expansion contract red

Focused daemon search regressions on the separate `:5434` postmaster failed in `search_scope_starvation`:

- `a_scoped_search_is_not_starved_by_a_crowded_neighbour_repository`: expected 10, got 0;
- `llm_repro_queries_return_scoped_hits_without_the_lexical_leg`: expected 10, got 0;
- `a_scoped_search_returns_the_whole_limit_not_whatever_survived`: expected 1, got 0.

The other five tests in that binary passed. No production `:5433` work occurred. Per packet, this is a red existing-contract tripwire, so I stopped without rerunning or changing expansion policy.

## Source-level diagnosis for ruling

[INFERENCE] The post-filter design loses `candidate_count` exactly when admission removes the entire first HNSW page. The SQL cross-joins `candidate_meta` onto `nearest`; with zero admitted rows, the result set is empty, `candidate_count(&rows)` defaults to zero, and `scanned < candidate_limit` returns empty immediately instead of expanding. The expansion loop therefore never requests the next HNSW page.

Smallest contract-preserving repair: make the SQL return one internal sentinel row carrying `candidate_count` when `nearest` is empty (for example, candidate_meta LEFT JOIN final hits), include nullable `element_id` internally, and have Rust exclude only that sentinel while using its count to decide expansion. This keeps the existing expansion constants and HNSW-first plan unchanged.

Please rule whether to implement that sentinel/count repair. No code or further tests until ruled.
