## Summary

- make ordinary queue depth live-only and index-only; retain full history only behind `flowspace3 status --history`
- purge done jobs older than the configured one-day window at boot and hourly in bounded 10,000-row statements, with a durable `/status` and log receipt
- make failed non-terminal jobs retain their dedupe key and migrate dirty existing duplicates safely

## Measured basis

Prod had 1,016,092 jobs, including 1,009,934 done rows (99.4%). The old `queue_depth()` used a three-process Parallel Seq Scan, read 892 MB (`hit=17 read=114185`), and cost about 260 ms CPU every ~6.5 seconds. Done rows grew by at least 515k/day; the ruled one-day default avoids retaining roughly 3.6M rows under the original seven-day proposal.

## Proof

All DB-backed tests below used only the dedicated `127.0.0.1:5434/flowspace3_test` postmaster and per-test databases.

- store retention/plan/dedupe contract file: 3 passed
- migration 0023 dirty-backfill/index contract: 1 passed
- daemon `status_retention`: 2 passed
- runner progress log: 1 passed
- runner queue snapshots: 1 passed
- CLI status/history rendering and parsing: 4 passed
- config validation/default: 1 passed
- retention cadence: 1 passed
- `harness checks`: green on the dedicated `:5434` test postmaster

Actual CLI smoke: `flowspace3 status --help` exposes `--history`; `status --watch --history` fails closed as mutually exclusive.

## Mutation receipts

1. The queue-plan test also runs the old unfiltered `GROUP BY` golden against 200,000 done rows and asserts that it *does* contain a jobs `Seq Scan`; the production live query must contain `Index Only Scan` on `jobs_live_dedupe_idx` and no jobs `Seq Scan`.
2. Migration 0023 starts from the old predicate with duplicate failed non-terminal rows, converges to one owner, pins the broader partial-unique predicate, and proves another failed-owner insert is rejected. The mint-flow test proves the original failed row absorbs a re-fire and remains the sole owner.

## Production handoff

A normal post-merge daemon bounce runs `RetentionSupervisor` immediately. Its boot pass repeatedly calls `purge_done_jobs` in 10,000-row statements until fewer than 10,000 remain, then records/logs the complete receipt. O-prime owns the bounce and ac-0005 before/after counts, relation size, three status timings, and five-minute `seq_tup_read` delta; no hand-written DELETE is needed.

## Assumptions

- daemon migrations complete before the runner/reconcile roster starts
- `updated_at` is the retention age: `complete_job` sets it at settlement
- failed non-terminal rows remain failed when a re-fire is absorbed; boot recovery remains the transition back to pending
- the shared reconcile cadence remains five seconds, making 720 retention ticks one hour
- explicit historical status may pay a full-history aggregate; no hot path calls it
