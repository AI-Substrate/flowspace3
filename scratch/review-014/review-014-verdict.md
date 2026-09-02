# review-014 DELTA VERDICT — cross-model re-review

**VERDICT: APPROVE** — all three findings fixed, each independently re-derived and mutation-checked.

- **Delta sha**: `c5242eaab4bfaeb9f0cb324d395e36702650cab4` (`fix: revive failed jobs on re-enqueue`, PR #98 head)
- **Round 1 sha**: `cc8da52c2595662664490de7d2b7a120cad95beb`
- **Delta scope**: 9 files, +182/−37. Code touched: `jobs.rs` (+9), `0023_jobs_retention.sql` (+6), `pg_jobs_retention.rs` (+140 tests), `daemon.md`, plus plan/tasks docs. Nothing outside the fix.
- **Record**: `docs/plans/014-jobs-retention/assets/reviews/cross-model-review.dd.json` (rebuilt, 0 warnings; new `delta` section, d-001…d-007)
- **CI**: `gate` was **PENDING** on `c5242ea` at review time — read, not rerun, per instruction.
- **Environment**: `:5434` only. Prod `:5433` / `:7373` never touched. No full `harness checks`.

## Findings — all fixed

| id | severity | status |
| --- | --- | --- |
| f-001 | CRITICAL | **FIXED** — verified |
| f-002 | MEDIUM (doc) | **FIXED** — verified |
| f-003 | MEDIUM (code) | **FIXED** — verified |

### f-001 — cured

I rebuilt the scratch databases **from scratch** off the amended migrations first, having dropped the index I hand-built in round 1 and re-confirmed the pre-fix baseline still replanned to `Seq Scan` — so what I measured is the coder's migration, not my own artefact.

Ran the exact post-fix SQL (`jobs.rs:127-137`) against the **worst realistic stuck state**: failed non-terminal, `attempts=3` (at `MAX_ATTEMPTS`), `parks=20` (at `MAX_PARKS`).

| | round 1 (`cc8da52`) | delta (`c5242ea`) |
| --- | --- | --- |
| rows after re-add | 1 | 1 |
| state | `failed` | **`pending`** |
| attempts / parks | 3 / 20 | **0 / 0** |
| `claim_job` | `-1` (dark forever) | **`1` — claimable** |

### f-003 — cured

Fresh 200k-done-row seed. With the migration's index: `Limit → Index Scan(jobs_failed_recent_idx)`, cost **4.14**. Dropped it: `Limit → Sort → Seq Scan`, cost **5167.16**. Restored: `Index Scan`. So the new assertion is genuinely capable of failing.

The query still returns a **terminal** failure (`t1 / malformed payload`), which confirms the `daemon.md` correction is accurate — latest failure stays visible in ordinary status; only terminal *counts* need `--history`. My 5167.16 vs the coder's reported 5358 is seed variance, same order.

### f-002 — cured

Plan goal now reads *"default 1 day, ruled 2026-09-02 in prime-reply-001 item 4"*. **ac-0003 was additionally amended** to require the absorbed re-fire be claimable — that closes the under-specification that let f-001 hide behind a green test, which matters more than the doc line itself.

## Mutations — I re-ran all three of the coder's, and added one they did not

| mutation | result |
| --- | --- |
| `state` CASE removed | k_failed stays `failed` — **f-001 returns** ✅ red |
| `attempts` reset removed | revived at `attempts=3` = `MAX_ATTEMPTS`, fails on first retry ✅ red |
| `parks` reset removed | revived at `parks=20` = `MAX_PARKS`, `verdict()` can never park it again ✅ red |
| **mine**: unguarded `state='pending', attempts=0, parks=0` | **demotes the RUNNING control** and wipes a live pending row's budgets |

The fourth is the one worth keeping: it proves the `CASE` guard is load-bearing and that the naive form of this fix would have shipped a new defect.

## Controls — nothing else disturbed

revivable-failed → revived · pending → budgets **preserved** (1, 2) · running → **stays running**, budgets preserved, not demoted · terminal-failed → **not absorbed**, fresh claimable row minted beside the retired defect row. Zero keys with more than one live owner, so the unique index, the `requeue_failed` guard removal and ac-0003 all remain valid.

`ac-0001 not regressed`: live census still `Aggregate → Sort → Index Only Scan(jobs_live_dedupe_idx)`, no `Seq Scan`.

## Tests — now assert the contract instead of pinning the defect

The round-1 test asserted `state == "failed"`. The replacement **actually claims the row** via `claim_job` — behavioural, not a row inspection — and the new `dedupe_running_and_terminal_failed_rows_keep_their_distinct_semantics` pins both controls I asked for. Suites re-run by me at the fix sha, **exit 0**: `pg_jobs_retention` 4 passed, `pg_migrations` 9 passed including `migrating_twice_changes_nothing`.

## n-005 — PRECONDITION for the bounce (new; read before restarting prod)

The fix **added the index to migration 0023** rather than cutting a 0024, so 0023's content — and its checksum — changed after it had already been applied somewhere. sqlx validates this: `sqlx-core-0.8.6/src/migrate/migrator.rs:175-176` compares checksums and returns `MigrateError::VersionMismatch`.

Any database that already ran the **pre-fix** 0023 will now refuse to migrate, and the daemon will fail to start — the same class of failure `StoreError::Migrate`'s own doc records from 2026-08-27. Per-test databases are created fresh, so **the suite cannot catch this**.

Before the bounce: confirm no environment (dev box, the coder's own DB, any staging copy) carries pre-fix 0023 in `_sqlx_migrations`. If one does, drop that database or re-cut the index as 0024. Reported as a precondition, not a finding, because 0023 was explicitly ruled free to grow while unshipped.

## Remaining — o-prime's

ac-0005's prod receipt after merge + bounce, subject to **n-001** (receipt is transient — capture within the hour or use the log line), **n-002** (non-concurrent index build pauses first boot), and **n-005** above.

**Bottom line**: the plan's promises are now true as written. The hot path is index-only on both its reads, retention is bounded and safe, dirty duplicates converge without losing a job, and a failed job's re-fire actually re-fires. Approve.
