# Worker brief — schema v1: migrations + typed store API · (seat at canary, pane %41)
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · one bounded task

## The job
Turn **workshop 002** (`docs/plans/prd/workshops/002-pg-schema.md` — AUTHORITATIVE; decisions D1–D8 are settled, do not re-litigate) into the real database: sqlx migrations + the typed store API ("our simple ORM") + docs. This is the substrate the daemon plan combines everything into.

1. **Migrations** (forward-only, `crates/store/migrations/` — read `docs/services/database-migrations.md` first; NEVER edit applied files): implement the workshop DDL as `0003+` — **mollusk owns `0002_element_kinds.sql`** (kind-spelling fix, landing imminently). If 0002 isn't on main when you're ready to land, message me. Note the workshop's elements shape supersedes parts of 0001 — write honest ALTERs/rebuilds, don't pretend 0001 didn't happen. Workshop open questions: assume the sketch's defaults (D3 per-dim tables — implement `embeddings_1024` only for now; raw_text inline; GC deferred) unless I message otherwise.
2. **Typed store API in `fs3-store`** (this is the "ORM" — sqlx stays behind the store's API, arch-enforced; no ORM crate): row structs + functions per flow, not table-CRUD for its own sake: `upsert_element_tree`, `get_elements(blob_sha, parser_version)`, `put/get_smart_content(raw_hash, model_key)`, `put/query_embeddings` (HNSW similarity query returning joined element+smart rows), `enqueue_job(kind, dedupe_key, payload, not_before)` (upsert semantics per D1), `claim_job()` (**`FOR UPDATE SKIP LOCKED`** per D4), `complete/fail_job`, `missing_enrichment(model_key)` (the D6 reconciler query). `pgvector` crate for vectors.
3. **Tests** against compose PG :5433, throwaway-db isolation (copy `pg_migrations.rs`'s pattern): fresh bootstrap through ALL migrations; element-tree round-trip with parenting; content-address dedupe (two elements, same raw_hash → one smart row); job lifecycle incl. debounce-push and two concurrent claimers never taking the same job; similarity query returns nearest-first (use testkit's deterministic fake embeddings).
4. **Docs**: `docs/services/store-schema.md` (convention: `docs/services/README.md`) — the schema map, the flows, the claim pattern, verify commands.

## Rules & fence
- Architecture: `docs/rules-idioms-architecture/fs3-architecture.md`. sqlx/pgvector ONLY in fs3-store. No mocks. No new ports.
- Fence: `crates/store/**`, `docs/services/store-schema.md`. Element MODEL is mollusk's — consume whatever `fs3_core::Element` looks like when you integrate (its cutover may land mid-task; adapt, don't fight). If the tree doesn't compile at start, work per-package.
- Commit+push per unit, scoped adds, push-first (ruling 2026-08-26-commit-push-as-you-go.md). Gates: `harness checks` + `cargo test -p fs3-store` (workspace once green). Report to pij-instant-lynx. Deviations = stop-and-ask.
