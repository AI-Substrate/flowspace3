# Phase 1 execution log

## tk-0101 — serialise create/drop

- Initial implementation placed a process-wide semaphore in testkit; review delta f-1a01 superseded that placement with the store-level `FS3_DB_MUTATION_CONCURRENCY` boundary.
- Guarded every `fs3_store::create_database` / `drop_database` call made by `FreshDatabase`, including orphan sweep and cleanup.
- Proof: `cargo test -p fs3-testkit serialis` passed (`artifact://25`). Removing the permit made the test fail with `observed more than 1 concurrent database mutations` (`artifact://27`); the guard was restored and read back at `fresh_database.rs:42-48`.
- The test uses independent current-thread Tokio runtimes, proving the static semaphore coordinates across runtimes rather than relying on one test runtime.

## tk-0102 — truthful failure advice

- Added end-to-end panic tests for a refused port and a TCP listener that accepts then closes PostgreSQL connections.
- SQLx collapses a refused connect attempt into `PoolTimedOut`, losing the underlying `ConnectionRefused`. The failure path therefore performs one bounded 250 ms TCP reachability probe after database creation fails: a confirmed refusal names the start command; an answering socket or recovery signature says to wait and retry; unknown failures give neutral configuration/log advice.
- Proof: `cargo test -p fs3-testkit advice` passed both cases (`artifact://36`), including the absence of `docker compose up -d` when a server answered.

## tk-0103 — minted-name sweep and read-only listing

- The parser now accepts the helper's complete `fs3_<label>_<epoch>_<32hex>` shape, including `fs3_test_…`, while enforcing the helper's 12-character alphanumeric label bound and exact entropy tail. `flowspace3` and `flowspace3_test` remain ineligible.
- Added `FreshDatabase::list_orphans_from`, returning sorted `OrphanCandidates` without mutations. `sweep_orphans_from` consumes the same candidate logic and serialises each drop.
- The shared test server already contains older unrelated test databases. The integration test uses a synthetic near-epoch clock so exactly its two epoch-1 databases are old; this prevents ambient state from entering the candidate set. A failed exploratory run's three `sweep012`/known `test` databases were removed explicitly.
- Proof: the shared-server command in the task receipt passed both sweep tests (`artifact://45`); list-only found both minted shapes, sweep removed those two, and the fresh database remained.

## tk-0104 — default-parallelism oversize proof

- Window began `2026-09-02T00:54:34Z` against the shared test server at `127.0.0.1:5433`.
- `cargo test -p fs3-daemon --test oversize` ran without a thread override: 12 passed in 13.81 s (`artifact://59`).
- `docker logs --since 2026-09-02T00:54:34Z flowspace3-db` contained zero postmaster termination, interruption, recovery-mode, or ready-after-recovery signatures.

## tk-0105 — gate and PR

- `harness checks` passed every mandated gate at `2026-09-02T01:05:54Z`.
- `harness commit` created verified commit `5c7f7bdb069cdb79de3bcf2203d57f934a75c22c`; its `refs/notes/ai` attribution landed.
- PR #95 opened against `main`, with the lock-removal mutation and all behavioral receipts in the body. GitHub gate run `33578074863` passed in 5m5s.

## Review delta f-1a03 — live database safety

- Added `idle_database_names_with_prefix`, filtering `pg_stat_database.numbackends = 0`, and `drop_database_if_idle`, which rechecks liveness then uses an unforced drop so a racing connection preserves the database.
- The sweep test holds a real pool open on an aged `fs3_sweeplive_…` database: list-only excludes it, direct idle-drop returns false, sweep removes both idle names, and the live database remains.
- Proof: sweep tests green (`artifact://113`). Removing the catalog liveness predicate made the live name enter the candidate list and failed the test (`artifact://106`); the predicate was restored. Failed-mutation scratch databases were explicitly removed.
- Refined proof for the post-list race: a candidate is listed idle, then connected before the shared drop loop. Per-drop liveness recheck under the store permit skips it, unforced DROP preserves the session, and the connection remains usable (`artifact://154`). Replacing that path with forced drop killed the racing database and failed (`artifact://152`).

## Review delta f-1a01/f-1a04 — store-level lock and create-path proof

- Moved the process-wide semaphore into `fs3_store` and renamed its clean-cutover setting to `FS3_DB_MUTATION_CONCURRENCY` (default 1, accepted range 1–2). Store create, forced drop, and idle drop each acquire exactly once.
- Removed all testkit-side wrappers in the same change to avoid N=1 self-deadlock. The store integration helper now routes its raw CREATE/DROP through the shared primitives.
- Added an N=8 real `create_database` test with a test-build in-flight counter after permit acquisition. Green: one maximum (`artifact://117`). Removing the permit from the create path produced eight concurrent CREATE operations and failed; all databases were cleaned before the assertion.
- Added a 10-second timeout around the multi-drop sweep; it passed after the one-layer cutover (`artifact://120`).

## Review delta f-1a02 — permanent authentication failures

- Added an authentication/permission branch ahead of recovery advice, recognizing PostgreSQL invalid-password/insufficient-privilege codes and messages.
- A real bad-password URL now says to fix credentials, without compose or wait/recovery advice; the refused-port and accept-then-close behavior remains unchanged.
- Proof: three advice tests green (`artifact://123`). Disabling the auth branch made only the bad-password contract fail and reproduced the misleading recovery advice (`artifact://125`); the branch was restored.

## Review delta ac-0005 seam — read-only listing example

- Added `cargo run -p fs3-testkit --example list_orphans -- <base-url>`, which calls only `FreshDatabase::list_orphans_from` and prints one candidate name per line.
- Against the shared `:5433` test server, the example completed successfully; `fs3_%` database count was 48 before and 48 after, proving the invocation was non-destructive. It printed no currently eligible idle candidates.
- The first pre-count coincided with o-prime's ruled row-140 PostgreSQL restart and correctly triggered stop-and-ask 001. After reply 005 confirmed the server healthy, the proof resumed once and passed.

## Review delta — scope correction

- Plan and implementation-guide summaries now state the real boundary: serialization is per process and covers callers using `fs3_store` primitives. Separate seats can still issue concurrent DDL against the shared postmaster.
- Backlog row 126 is therefore REDUCED, not closed. The separate test postmaster in row 124b remains its own packet.

## Review delta — corrected checkpoint proof

- Default-parallel `cargo test -p fs3-store` passed 137 tests, 4 ignored, in the `2026-09-02T01:48:52Z..01:53:07Z` window (`artifact://161`); recovery/termination signatures were zero.
- Report-only measurements: 83 `immediate force wait` log starts and `pg_stat_bgwriter.checkpoints_req` 1171→1299 (+128), versus the reviewer's 25 over 38 seconds.
- Stop-and-ask 002 correctly halted on the then-binding reduction target. Reply 007 ruled that target invalid: every DROP forces a checkpoint; serial execution removes overlap and therefore can reduce coalescing and increase line count. The gate is zero recovery plus the mutation-checked N=8 concurrency bound; checkpoint counts remain context, not verdict.

## Review round 3 — residual fold-ins

- Routed the last raw DDL sites in `crates/store/tests/pg_first_light.rs` and `crates/daemon/tests/support/mod.rs` through `fs3_store::drop_database`; exact statement search now finds SQL execution only in `store/src/admin.rs`.
- Widened orphan parsing to the legacy `fs3_migrations_<32hex>` and `fs3_storelock_<32hex>_<worker>` layouts. Their lower 64 seed bits preserve Unix nanoseconds, so the existing age policy applies. The parser test passed (`artifact://49`), failed when the migrations branch was removed, and returned green with the full sweep set (`artifact://84`).
- Replaced phrase matching with the binding SQLSTATE-class rule: class 57 remains transient/recovery; other database errors name credentials, permissions, or configuration. Three advice paths passed (`artifact://70`); disabling the permanent branch reproduced the bad-password recovery lie (`artifact://62`).
- Moved create overlap instrumentation into the test-only `create_test_hook` module and restored the eight-OS-thread/eight-independent-current-thread-runtime regression beside the N=8 single-runtime test. Both passed (`artifact://68`); removing the store create permit made both fail at 16 observed concurrent creates (`artifact://60`).
- Promoted the existing `fs3-store -> tokio` architecture edge from dev-only to shipped with o-prime approval: the store semaphore is runtime behavior, while SQLx already brings Tokio into the binary. `fs3-arch-check` passed with 8 crates, 91 direct edges, and 0 violations.
- Extracted the exact unforced sweep SQL template and added `orphan_sweep_drop_statement_is_unforced`. Adding `WITH (FORCE)` made the guard fail (`artifact://78`); restored sweep tests passed (`artifact://84`). AC-0003 now distinguishes M1/M2a/M2c mutation coverage from this defence-in-depth SQL guard.
- Added a standard-library drop-on-exit guard for the five synthetic-epoch sweep databases. Under the M1 mutation the integration test failed as expected (`artifact://81`), then the guard left zero matching databases on `flowspace3-db-test`; the restored suite passed (`artifact://84`).
- Attributed `pg_stat_activity` probes on `flowspace3-db-test` with `application_name=rs-review-012` closed the raw-DROP residuals: store `pg_first_light` baseline 2 → **1** (239 samples, 143 with DDL, 0 over 1); daemon `first_light` baseline 2 → **1** (618 samples, 143 with DDL, 0 over 1). The full default-parallel store suite measured **1** maximum across 1631 samples, 1332 with DDL, 0 over 1, exit 0. Foreign DDL peaked at 16 but was excluded by attribution.
