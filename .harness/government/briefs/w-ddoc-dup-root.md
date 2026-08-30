# w-ddoc-dup-root — one blob, two file roots: writer defect + brittle readers

**From**: pij-instant-lynx · 2026-08-30 · Jordan ruled dispatch. Closes
backlog row 73 (escalated).

## The defect (two, actually — prod-live)

Blob `402efae15bbc152d0388eae2ad9baa2bd0a65c14` has **2 file roots where
exactly 1 is expected**. Consequences observed in prod:
- ~20 `scan_file` jobs FAILED with the invariant violation as last_error
  ("row is not a valid element: invalid config: blob 402efae… at pa…").
- `flowspace3 status` itself hard-failed (FS3-E-STORE-QUERY-FAILED) when
  the query crossed the dirty rows (bedbug's sighting) — a READ verb
  broken by dirty data.

Two defects, both in scope:
1. **The writer**: find what minted a second root for one blob (ddoc scan
   path, 008 family — suspects: ddoc + generated sibling, re-scan across a
   parser/tooling change, worktree mapping edge post-#69). Root-cause with
   receipts from the prod rows (READ-ONLY SQL), then fix the invariant at
   the write site: one blob → one file root, enforced (constraint or
   upsert shape), not assumed.
2. **The readers**: a status/read query must never hard-fail on a data
   inconsistency — it reports the inconsistency (which blob, which paths,
   next_action naming the repair) and completes. Walk this to every reader
   that shares the shape (tenet 16).

Plus repair: a migration or maintenance path that heals existing dup-root
rows (deterministically pick/merge the survivor, requeue affected scans),
so prod's 20 failures clear after the fix lands.

## Rules & fence

- READ-ONLY against prod DB until root cause is NAMED with evidence, then
  STOP-AND-ASK o-prime before any repair touches prod (the heal runs as
  migration/verb after merge, on the bounce — not as a live hand-edit).
- IN: ddoc/scan write path, the failing readers, migration/repair, tests
  (fixture that MINTS the dup and proves both the writer refuses and the
  reader reports). OUT: search/ask surfaces, lexical channel, queue.
- Worktree fs3-ddoc-dup-root; plan-ack before code; per-seat
  CARGO_TARGET_DIR; base FS3_TEST_DATABASE_URL as server selector; never
  test against prod :7373; harness checks/commit; PR into main.
