# jobs-retention coder report

## Head

- branch: `014-jobs-retention`
- head: `c5242eaab4bfaeb9f0cb324d395e36702650cab4`
- implementation commit: `cc8da52c2595662664490de7d2b7a120cad95beb`
- review-fix commit: `c5242eaab4bfaeb9f0cb324d395e36702650cab4`
- PR: https://github.com/AI-Substrate/flowspace3/pull/98

## Shipped

- Default `queue_depth` is live-only: pending, running, failed non-terminal. Its 200k-done-row plan is pinned to `Index Only Scan` on the covering `jobs_live_dedupe_idx`; the old unfiltered GROUP BY golden is pinned red with a jobs Seq Scan.
- `flowspace3 status --history` is the only full-history escape hatch. Ordinary `/status`, progress logging, shutdown reporting, TUI snapshots, and queue events use live rows.
- `indexing.job_retention_days` defaults to the ruled 1 day and rejects zero. `RetentionSupervisor` runs at boot and hourly, deleting old done rows in indexed 10,000-row statements and recording a durable status/log receipt.
- Migration 0023 tolerates existing failed non-terminal duplicates by retaining one active owner and terminalising redundant failed rows. The broader partial unique index plus matching enqueue conflict target makes a second mint impossible; an absorbed failed re-fire revives that owner to pending with fresh attempt/park budgets and is claimable. Running work is preserved; terminal history permits a fresh row.
- Periodic progress logs retain `scan_left`, `summarize_left`, `embed_left`, and `failed`; done-derived cumulative fields are removed by ruling.
- User/reference/service docs and deterministic task receipts match the new contract.
- `jobs_failed_recent_idx` serves `/status`'s latest-failure query without scanning completed history.

## Evidence

All DB-backed evidence used only `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test` and per-run child databases.

- store retention/queue-plan/dedupe: 3 passed
- migration 0023 backfill/index: 1 passed
- daemon status receipt/history + purge log: 2 passed
- runner progress log: 1 passed
- runner live snapshots: 1 passed
- CLI status parsing/rendering: 4 passed
- config default/validation: 1 passed
- retention boot/hourly cadence: 1 passed
- actual CLI smoke: `status --help` exposes `--history`; `--watch --history` fails closed
- `harness checks`: GREEN, every gate, dedicated `:5434` postmaster
- PR implementation-head CI: GREEN in 5m22s
- final receipt-only head CI: GREEN on rerun in 4m22s; initial attempt hit an unrelated GitHub Copilot fixture race
- review delta: state CASE mutation red (`artifact://136`), attempts reset mutation red (`artifact://154`), parks reset mutation red (`artifact://138`), latest-failure index mutation red with jobs Seq Scan cost 5358 (`artifact://150`), restored focused suites 13/13 green (`artifact://156`)

## Production invocation

After merge, the normal o-prime-owned daemon bounce is the invocation. The boot tick constructs `RetentionSupervisor`, repeatedly calls the shipped `purge_done_jobs` with a 10,000-row batch until the final short batch, then records/logs the complete receipt. Do not hand-write a DELETE.

O-prime then records ac-0005: before/after state counts; `pg_total_relation_size('jobs')`; three `flowspace3 status --json` wall timings under 200 ms; five-minute `pg_stat_user_tables.seq_tup_read` delta.

## Assumptions

- Daemon migrations finish before the runner/reconcile roster starts.
- `updated_at` is the retention age; settlement updates it.
- A failed non-terminal row revives to pending when a re-fire is absorbed; boot recovery remains the fallback for enrichment failures that receive no new enqueue.
- The shared reconcile cadence stays five seconds; 720 ticks is one hour.
- Explicit historical status may pay a full-history aggregate; no automatic or hot path calls it.
- Deleting retained rows creates dead tuples; the before/after relation-size receipt may stay flat until ordinary vacuum reclaims space. No VACUUM behavior is hidden inside retention.

## Deviations and noteworthy findings

- Original plan text said 7 days. O-prime ruled 1 day from measured churn of at least 515k done rows/day (about 3.6M retained at 7 days); task/config/docs were updated to 1.
- The active dedupe index required covering columns to make grouped live depth genuinely index-only; migration 0023 carries both the dedupe and plan-shape fixes.
- The shared `:5433` database crashed during the packet. Final evidence uses only the dedicated `:5434` test postmaster; setup frictions are recorded in the shared harness observation buffer.
- `.serena/` is tool-created untracked state and is not part of either commit.
- Observation buffer was listed, not cleared, after the review-fix push. It includes `DL-009`: `harness commit` reported connected ingress but the `refs/notes/ai` verification was missing; o-prime is carrying this to the harness prime.
