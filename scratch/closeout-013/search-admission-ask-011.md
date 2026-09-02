# search-admission STOP-and-ask 011 — f-9c41 requirements conflict mechanically

I read the full reviewer verdict and reply 016. Two requested properties cannot both hold with the ruled HNSW-first post-filter architecture:

1. Paired geometry: 12,000 foreign vectors are nearer than 5 scoped vectors; limit 10; return all 5 in at most 2 Rust/SQL passes.
2. `admitted_elements` is bounded to the raw hashes in the current candidate page, while candidate_vectors is the unfiltered HNSW page and admission occurs above it.

With candidate limits 40 then 80, neither page can contain any of the five scoped vectors behind 12,000 nearer vectors. A growth sentinel can stop after pass 2, but the only truthful short result is 0, not 5. Returning 5 within 2 passes requires restoring repository/worktree/path admission as a filter *inside* the HNSW scan, letting pgvector iterative scan skip the 12,000 foreign vectors internally. That supersedes “every admission join sits above the HNSW node” and makes the paired test independent of the admitted-growth mutation (removing growth still returns 5 on pass 1 because candidate_count=5 < 40).

A second fence/API decision is needed: “short page with empty_because/meta saying candidate_limit_exhausted” requires changing `fs3_store::search_elements` from `Vec<SearchHit>` to a page result carrying `hits`, `passes`, and `candidate_limit_exhausted`, then changing `crates/daemon/src/search.rs` and HTTP meta. Daemon files are outside my packet fence, but reply 016 appears to authorize that clean cutover.

Please rule one coherent contract:

A. **Preserve HNSW-first post-filter:** paired test expects `Ok` with 0 hits in ≤2 passes and `candidate_limit_exhausted` meta; removing growth makes it run 9 passes (red). This satisfies bounded admitted-elements and discriminates the growth check.
B. **Return 5 hits in ≤2 passes:** restore scope admission inside the HNSW scan using pre-resolved source keys; relax the “all admission above HNSW” shape assertion and use a separate geometry to mutation-check admitted growth.

Also confirm that reply 016 authorizes edits to `crates/daemon/src/search.rs` and `crates/daemon/src/http.rs` for the exhaustion metadata clean cutover.

No code changes for review fixes until this conflict is ruled. Existing unpushed changes are only task receipts and the already-reviewed fixture stabilization.
