# w-settle-hotpath — take queue observability off the per-job settlement path

**From**: pij-instant-lynx · 2026-08-30 · Remediation #1 of
`scratch/scan-throughput-review.md` (read it first — the measured case).

## The defect (measured)

Every completed general job runs `jobs_remaining` then a full grouped
`queue_depth` aggregate (`crates/daemon/src/runner.rs:781-804`); embed
settlement does the same per original job row (runner.rs:595-617). On the
prod-shaped table (544,721 rows / 1.5 GB) the aggregate measured **80.84 ms
and 60,574 buffers — per settled job**. Progress logging already has a
five-second cadence (runner.rs:693-730); the per-job census is pure waste.
In the 959-file run, handler work was ~478 worker-seconds but wall was 722s
— this is the biggest known contributor to that gap.

## The job

1. Remove `queue_depth` (and any full-history scan) from per-job settlement.
   Settlement keeps its UPDATE and, if needed, the cheap indexed live count
   (measured 0.241 ms).
2. Queue-depth snapshots move to the existing five-second reporting cadence
   (or an equivalent timer/delta scheme) — the operator-visible information
   stays, its cost stops scaling with jobs settled.
3. Prove it: a before/after measurement on a seeded large jobs table (test
   or bench receipt in the PR) showing settlement no longer performs the
   grouped aggregate; existing progress output still appears.

## Fence

IN: runner.rs settlement + emit cadence, store query call sites, tests.
OUT: claim index (w-claim-index), batching (w-embed-microbatch), schema
changes, LIFO ordering. Standard rules: own worktree fs3-settle-hotpath,
plan-ack before code, harness checks/commit, PR into main, never prod :7373.
