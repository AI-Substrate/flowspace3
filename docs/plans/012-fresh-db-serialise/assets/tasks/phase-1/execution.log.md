# Phase 1 execution log

## tk-0101 — serialise create/drop

- Added one process-wide `tokio::sync::Semaphore`, initialized once from `FS3_TEST_DB_CONCURRENCY` (`1` by default; accepted range `1..=2`).
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
