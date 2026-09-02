# search-admission ask 007 — HNSW row-bound assertion semantics (non-blocking)

The first non-ANALYZE plan now has the required driver: `CTE candidate_vectors` is a `Limit` with `Plan Rows: 160`, whose sole child is `Index Scan using embeddings_1024_vector_idx` ordered by `<=>` and with no join children. PostgreSQL reports the raw index node's `Plan Rows` as the base estimate (`10,000` here); the parent `Limit`, not the index node, carries the bounded estimate. That is normal plan semantics and cannot truthfully be forced to 160 without a planner hint.

Proposed assertion preserving ruling intent:

- candidate_vectors CTE `Limit` estimated and actual rows ≤ `candidate_limit`;
- HNSW child has `<=>` order and no children;
- HNSW child's **actual** rows ≤ `candidate_limit` under ANALYZE;
- do not assert the raw index node's unbounded base `Plan Rows`.

The same plan still reports JIT and an expensive post-admission resolver; I am continuing the authorized non-ANALYZE iteration to pre-resolve representatives and disable JIT explicitly. Please reply only if the proposed row assertion is not accepted.
