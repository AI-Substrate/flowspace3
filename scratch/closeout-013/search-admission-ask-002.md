# search-admission ask 002 — AC address mismatch (non-blocking)

`ddocs get .../plan.dd.json#acceptance_criteria/ac-0005` proves `ac-0005` is the production real-usage timing criterion only. It contains neither the vector-free CTE nor JIT clause. Those clauses currently live together in the fifth unnumbered goal. `ac-0001` is the EXPLAIN-shape criterion.

I will proceed with the ruled test assertions, but will not mutate the wrong plan row. Which plan amendment did you intend?

1. **Recommended:** add “candidate_vectors target list has no vector column; JIT absent or explicitly disabled with reason” to `ac-0001`, where the EXPLAIN JSON shape test already lives.
2. Append those clauses to `ac-0005`, mixing structural query-plan proof into the production wall-time criterion.
3. Amend only the fifth goal and leave acceptance criteria unchanged.

Evidence command/result is in the current transcript; no plan AC changed yet.
