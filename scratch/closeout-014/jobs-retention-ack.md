# jobs-retention coder ack

CANARY-OK

## Identity

- pij id: `pij-chosen-arach` (`pij whoami`)
- spawnId: `s1788313099461-99670`
- model: `github-copilot/gpt-5.6-sol-fast-1m`
- seat: rs worker
- cwd/worktree root: `/Users/jordanknight/substrate/flowspace/fs3-jobs-retention`
- branch: `014-jobs-retention`
- o-prime: `pij-binding-magpie`

No code has been written.

## Measured basis

- Prod held 1,016,092 jobs: 1,009,934 `done` (99.4%), 1,267 pending, 34 failed, 33 running.
- `queue_depth()` performed a 3-process Parallel Seq Scan with `shared hit=17 read=114185`: 892 MB read, 134 ms wall / about 260 ms CPU per call, approximately every 6.5 seconds.
- The `/status` query shapes accounted for 27.2% of active samples. Over 65 seconds, `jobs` advanced by 30 sequential scans and 10,162,106 tuples read (155,931 tuples/s), dominating about 200 MB/s of database reads.
- `jobs_remaining()` already completes in 0.771 ms through an index-only scan; it is not the target.
- `jobs` is a 2,150 MB relation with 196,719 dead tuples (16.2%), has never autovacuumed, and was only 6,549 dead tuples below its default autovacuum trigger.
- Done history grew from about 495,000 to 1,009,934 during today's batch: at least 514,934 new done rows/day. At that lower-bound rate, a 7-day window retains about 3.60 million done rows. I recommend a 1-day default unless product history requirements justify 7 days.

## Every direct `queue_depth` caller

Cross-checked with Serena references and an exact workspace search after rust-analyzer incorrectly returned zero references.

1. `crates/daemon/src/status.rs:30` — `status::report`, the `GET /status` snapshot. It currently returns every state; downstream human status rendering can show `done` and retried-then-succeeded counts. This is a genuine current history consumer, but the approved goal says default status becomes live-only.
2. `crates/daemon/src/runner.rs:773` — `report_progress`. It currently reads `done` buckets for the `scanned`, `summarized`, and `embedded` log fields, live buckets for `*_left`, and failed buckets for `failed`; it also emits all rows as `EventKind::Queue`. This is a genuine current history consumer and needs an explicit contract ruling rather than silently emitting zero cumulative counts.
3. `crates/daemon/src/runner.rs:429` — shutdown reporter inside `run_until_shutdown`. It filters only `running`; live-only is exact.
4. `crates/store/tests/pg_first_light.rs:472` — `queue_depth_is_grouped_by_kind_and_state`. It asserts a `done` bucket and must be split/replaced to test live-only default plus explicit history.
5. `crates/daemon/tests/lanes.rs:629` — `first_shutdown_signal_finishes_in_flight_without_dequeueing_more`. It reads pending only; live-only is exact.
6. `crates/daemon/tests/lanes.rs:665` — `second_shutdown_signal_cancels_in_flight_without_claiming_more`. It reads running only; live-only is exact.
7. `crates/cli/tests/daemon_shutdown.rs:307` — `sandbox_session`. It reads running only; live-only is exact.

`crates/store/src/lib.rs:64` is a re-export, not a caller. The TUI does not call the store function; its queue-history meter filters snapshot/event rows to pending/running, while the human status table can render historical `done` rows.

## Numbered implementation plan

1. **Resolve the contract/fence conflicts below before edits.** Record o-prime's ruling in `.harness/temp/agent/jobs-retention-prime-reply-001.md`; do not widen scope or add a migration without it.
2. **Make depth live-only without losing a history escape hatch.** Change the default store query to pending/running plus failed-non-terminal rows; retain the old aggregate as an explicitly named history query. Migrate every direct caller deliberately. Replace the stale index comment.
3. **Make the hot query genuinely index-served.** The current `jobs_live_dedupe_idx` contains only `dedupe_key`; it cannot produce grouped `(kind,state)` plus `last_error` index-only despite the plan's stated goal. In the migration requested below, extend the active unique index predicate to failed-non-terminal and cover `kind`, `state`, `last_error`, and `terminal`, then shape the live aggregate to use it. Prove `EXPLAIN (FORMAT JSON)` contains no Seq Scan on `jobs` with at least 200,000 done rows; run the old GROUP BY golden as the required red mutation.
4. **Preserve truthful progress semantics.** Route shutdown and queue-event snapshots to live depth. For periodic progress, follow o-prime's ruling: recommended default is remove cumulative `scanned`/`summarized`/`embedded` fields from this hot poll rather than emit false zeroes; keep `*_left` and failed-non-terminal counts. Per-job completion logs/events remain the completion record.
5. **Add bounded retention.** Add validated `indexing.job_retention_days`; implement `purge_done_jobs(older_than, batch)` as short batched deletes selecting only old `done` IDs; add a daemon retention supervisor using the existing reconciler pattern, run once at boot and periodically, and store a thread-safe receipt (`window_days`, `last_purge_at`, `purged_last_run`) for status plus one log receipt per sweep.
6. **Prove purge safety.** Seed old/young done rows and pending/running/failed rows; prove exact deletion, live-row preservation, batch cap, and idempotent second run. Exercise concurrent daemon ownership through the supervisor/status seam, not prod.
7. **Prevent failed-key duplicate mint atomically.** Replace the partial unique-index predicate and matching `ON CONFLICT` target so pending, running, and failed-non-terminal rows all hold the key. Keep the existing failed row's state on absorb so `requeue_failed` remains the recovery path. Add the required red/green duplicate-mint mutation test and migration guard for pre-existing duplicates per o-prime's data ruling.
8. **Expose the receipt and optional history.** Extend the shared `StatusReport` contract and daemon response with `retention`; if scope is granted, add `flowspace3 status --history` and pass the explicit history request to the daemon. Update affected status/render/event tests and service docs only where the observable contract changes.
9. **Verify and deliver.** Run the four focused backpressure tests on an isolated per-run database at `:5433`; request the shared cargo/gate slot; run `harness checks`; record both mutations and EXPLAIN evidence; commit with `harness commit`; open the PR; hand prod purge/bounce and the four ac-0005 measurements to o-prime.

## Stop-and-asks / rulings required

1. **Migration fence:** concurrency-safe failed-key prevention requires changing `crates/store/migrations`; the same migration should make the active unique index covering for the promised index-only grouped depth query. Permission requested.
2. **Public envelope and CLI fence:** `/status` retention plus `status --history` requires at least `crates/core/src/views/status.rs`, `crates/daemon/src/http.rs`, and `crates/cli/src/main.rs`/status rendering and tests, all outside the stated fence; it also changes a public envelope. Permission and exact desired history transport requested.
3. **Progress contract:** `report_progress` currently uses done history for cumulative `scanned`/`summarized`/`embedded`. Recommendation: drop those cumulative fields from the periodic hot-path log and keep live left/failed fields. Confirm.
4. **Retention default:** measured lower-bound churn makes 7 days about 3.60M done rows. Recommendation: default 1 day; confirm 1 vs 7.
5. **Proof environment:** `harness boot --json` built successfully but degraded because compose service `db` is not running. Confirm the isolated `:5433` test-postmaster path and allocate the cargo/gate slot before tests/checks.

## Tripwires

- `queue_depth_plan`: any Seq Scan node on `jobs` at >=200k done rows means stop; old SQL must turn it red.
- `retention`: any non-old/non-done deletion, batch overflow, or nonzero second purge means stop.
- `dedupe_failed`: a second row under a failed non-terminal key means stop; old predicate must turn it red.
- `status_retention`: absent/stale receipt or missing purge log means stop.
