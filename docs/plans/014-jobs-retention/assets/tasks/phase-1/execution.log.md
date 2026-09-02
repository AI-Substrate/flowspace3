# Phase 1 execution log

## tk-0101 — live-only hot path

Changed the default queue census to pending/running/failed-non-terminal, retained the old aggregate behind `status --history`, removed done-derived cumulative fields from periodic progress, and added a covering active-job index. The plan test seeds 200,000 done rows, proves the production query uses `Index Only Scan` on `jobs_live_dedupe_idx` with no jobs `Seq Scan`, and proves the old unfiltered GROUP BY mutation does scan.

Evidence:

- `cargo test -p fs3-store --test pg_jobs_retention -- --nocapture` — 3 passed on the dedicated `:5434` test postmaster.
- `cargo test -p fs3-daemon --test streaming progress_is_reported_while_the_queue_is_still_draining -- --nocapture` — 1 passed.
- `cargo test -p fs3-daemon --test streaming queue_snapshots_follow_reporting_cadence_not_settlements -- --nocapture` — 1 passed.
- `cargo test -p fs3-cli status -- --nocapture` — 4 passed.

## tk-0102 — bounded retention

Added validated `indexing.job_retention_days`, ruled default 1 day; `purge_done_jobs` deletes only aged done rows in 10,000-row indexed statements. `RetentionSupervisor` runs at boot and hourly, drains all eligible batches, records a durable receipt, and logs one completed-sweep line. Default `/status` exposes the receipt and live queue; `--history` is explicit.

Evidence:

- Store contract test proves exact eligible deletion, protected states, two-row batch cap, and idempotent complete second sweep.
- `cargo test -p fs3-daemon status_retention -- --nocapture` — 2 passed; HTTP receipt/history and log receipt.
- `cargo test -p fs3-daemon the_first_pass_is_due_then_the_hourly_cadence_holds -- --nocapture` — 1 passed.
- `cargo test -p fs3-core job_retention -- --nocapture` — 1 passed.

Snap-in wiring is in `crates/daemon/src/retention.rs`; `crates/daemon/src/boot.rs` already applies it. A normal daemon bounce invokes the boot sweep and drains eligible rows through this code—no hand-written production DELETE.

## tk-0103 — failed-key duplicate prevention

Migration 0023 retires redundant failed owners without deleting history, expands the partial unique predicate to failed-non-terminal rows, and gives the live census a covering index. The matching enqueue `ON CONFLICT` target absorbs a re-fire into the existing failed row; boot recovery remains the state transition back to pending.

Evidence:

- `cargo test -p fs3-store migration_0023 -- --nocapture` — 1 passed; dirty pre-migration duplicates converge, the index shape is pinned, and a new duplicate is rejected.
- `dedupe_failed_non_terminal_job_absorbs_a_second_mint` — green in the 3-test store contract run; one row and the original id remain.

## tk-0104 — gate and PR

`harness checks` passed every gate against the dedicated `:5434` test postmaster: docs, lockfile, test-db guard, harness contracts, formatting, clippy with warnings denied, the full isolated test suite, architecture drift, and deterministic documents. The first red exposed the missing configuration-reference row; the focused `config_reference` suite passed after adding it. A later transient full-suite red reproduced green, and the final complete harness gate passed. PR #98 opened; its implementation head passed CI in 5m22s.

## Discoveries & learnings

| Tag | Finding | Decision |
| --- | --- | --- |
| Noteworthy | Measured churn is at least 515k done rows/day, so the plan's original 7-day default retained roughly 3.6M rows. | O-prime ruled 1 day in `jobs-retention-prime-reply-001.md`; code, config help, and tests use 1. |
| Noteworthy | The old active unique index held only `dedupe_key`; grouped live depth could not be index-only. | Migration 0023 covers `kind`, `state`, `last_error`, and `terminal`; the EXPLAIN JSON test pins the exact plan node. |
| Noteworthy | `/status` and `report_progress` genuinely consumed done history; a blind semantic swap would have emitted false zero cumulative counts. | History became explicit; periodic logs retain only live `*_left` and failed counts. |
| Noteworthy | The shared database crashed and the first dedicated test-postmaster boot lacked host pg_hba setup. | O-prime established `:5434`; final DB-backed evidence uses only that postmaster. Harness observations DL-005/DL-006 carry the setup gap. |
