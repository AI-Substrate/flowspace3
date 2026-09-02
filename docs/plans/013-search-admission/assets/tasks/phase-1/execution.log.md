# Phase 1 execution log — search admission

## tk-0101 — old-query parity golden

Added `crates/store/tests/search_admission.rs` and captured the current query's addresses and scores in `crates/store/tests/fixtures/search_admission_golden.json`. The fixture exercises limits 10 and 40 across repository, path, raw-source, smart-source, element-kind, and exact-conversation filters. It contains the two multiplicity hazards ruled by o-prime: one raw hash carried by three eligible elements, and one summary text hash mapped to two eligible raw hashes, in both code and conversation scopes.

Saved the old correlated admission fragment at `crates/store/tests/fixtures/search_admission_old.sql` for the EXPLAIN mutation check.

Evidence: `cargo test -p fs3-store --test search_admission search_parity -- --nocapture` — 1 passed. Test DB was a labeled, migrated `fs3_test_searchadmiss_*` database created beside the configured `:5433` endpoint and destroyed after capture.

## Discoveries & learnings

- **Noteworthy:** The checked-out query already projects only `(source_hash, source_kind, chunk_no, distance)` from `candidate_vectors`; o-prime amended `ac-0001` and goal 5 so the implementation asserts this existing property instead of claiming a removal.
- **Noteworthy:** Global `ddocs` is the canonical deterministic-document CLI. The builder instruction naming `node_modules/.bin/ddocs` is stale; `DL-003` records the blocking false path, and o-prime is fixing it.
- **Noteworthy:** Rust-analyzer returned zero references for exported `search_elements` despite verified callers. `DL-004` records the miss; exact-identifier search plus exact reads are the callsite proof for this packet.
