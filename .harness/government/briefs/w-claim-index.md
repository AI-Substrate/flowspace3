# w-claim-index — put `kind` in the pending-claim access path

**From**: pij-instant-lynx · 2026-08-30 · Remediation #3 of
`scratch/scan-throughput-review.md` (read it first).

## The defect (measured)

The claim index is partial pending `(priority DESC, id DESC)
INCLUDE (not_before)` WITHOUT kind (`crates/store/migrations/
0016_job_lifo.sql:8-12`); claims filter kind after walking it
(`crates/store/src/jobs.rs:156-172,211-232`). Measured: an EMPTY embed
`LIMIT 64` probe walked **29,838 pending non-embed rows in 14.759 ms /
29,902 buffers** — and that probe runs at the start of every general drain
cycle (runner.rs:229-238). Cost scales with backlog of OTHER kinds.

## The job

1. One forward migration: claim index becomes
   `(kind, priority DESC, id DESC) INCLUDE (not_before) WHERE
   state='pending'` (or one partial index per lane if measurement prefers
   it). Keep the LIFO semantics EXACTLY — Jordan-ruled ordering, only the
   access path changes.
2. Verify claim SQL uses it (EXPLAIN receipt in the PR): the empty-embed
   probe becomes an index-edge lookup; the general claim stays fast.
3. Prove ordering unchanged: existing LIFO tests still pass; add one that
   fails if kind-first ordering ever changes claim priority semantics.

## Fence

IN: one migration, jobs.rs claim queries, tests, EXPLAIN receipts.
OUT: settlement census (w-settle-hotpath), batching (w-embed-microbatch),
any queue semantics change. Standard rules: own worktree fs3-claim-index,
plan-ack before code, harness checks/commit, PR into main, never prod :7373.
