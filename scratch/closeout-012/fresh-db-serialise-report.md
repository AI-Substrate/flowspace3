# fresh-db-serialise coder report

## Head

- Branch: `012-fresh-db-serialise`
- Head: `09509b7525c770babd9aae2e974dd7bea62b28de`
- PR: https://github.com/AI-Substrate/flowspace3/pull/95
- Push: `05d7d87..09509b7`
- CI gate: pass on this exact SHA, 4m53s, run `33587110274`
- Local exclusive gate: not run. O-prime reply 021 ruled the exact-SHA CI gate equivalent and sufficient; the slot exists for pre-PR proof.
- Reviewer delta: pending on `09509b7`; branch is idle unless the reviewer finds something.

## Shipped

- `fs3_store::{create_database, drop_database, drop_database_if_idle}` share one process-wide Tokio semaphore configured by `FS3_DB_MUTATION_CONCURRENCY` (default 1, max 2). Store and daemon test helpers now route through it; executable CREATE/DROP SQL remains only in `crates/store/src/admin.rs`.
- Test-only create overlap instrumentation moved behind `create_test_hook`. Both N=8 same-runtime and eight-independent-current-thread-runtime tests exercise the real store create path.
- Permanent PostgreSQL database errors are classified by SQLSTATE class: class 57 remains recovery/transient; other database errors name credentials, permissions, or database configuration. Refused-port and accept-then-close behavior remains distinct.
- Orphan parsing covers canonical timestamped names plus legacy `fs3_migrations_<32hex>` and `fs3_storelock_<32hex>_<worker>` names while retaining age validation.
- Sweep listing selects only idle databases; each drop rechecks liveness under the permit and executes the shared unforced SQL template. `orphan_sweep_drop_statement_is_unforced` guards the real statement against `FORCE`.
- The sweep race fixture now has drop-on-exit cleanup, including panic/mutation paths.
- AC-0001 restores attributed server-side observation; AC-0003 states the exact mutation/defence-in-depth split; AC-0004 targets `flowspace3-db-test` on :5434; the obsolete row-124b non-goal is removed.
- Architecture allowlist promotes the existing `fs3-store -> tokio` edge to shipped behavior. SQLx already ships Tokio; this changes the allowed dependency kind, not runtime machinery.

## Evidence

- Architecture: `cargo run -p fs3-testkit --bin fs3-arch-check` → 8 crates, 91 direct edges, 0 violations.
- Serialization green: `cargo test -p fs3-store serialised` → 2 passed (`artifact://68`). Permit removed → both tests red at 16 concurrent creates (`artifact://60`); restored.
- Advice green: `cargo test -p fs3-testkit advice_` → 3 passed (`artifact://70`). Permanent branch disabled → bad-password test red with the old recovery lie (`artifact://62`); restored.
- Sweep/parser/guard green: `cargo test -p fs3-testkit sweep` → 4 passed (`artifact://84`).
  - Migrations parser branch removed → legacy parser test red; restored.
  - Candidate liveness predicate removed → integration test red (`artifact://81`); drop-on-exit guard left 0 matching synthetic-epoch databases; restored.
  - `WITH (FORCE)` added to the executed SQL template → no-FORCE guard red (`artifact://78`); restored.
- Attributed DDL probes, `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test?application_name=rs-review-012`, `CONTAINER=flowspace3-db-test`:
  - store `pg_first_light`: baseline 2 → 1; `exit=0 samples=239 max_concurrent_ddl=1 samples_with_ddl=143 samples_over_1=0 foreign_ddl_max=15`
  - daemon `first_light`: baseline 2 → 1; `exit=0 samples=618 max_concurrent_ddl=1 samples_with_ddl=143 samples_over_1=0 foreign_ddl_max=16`
  - full default-parallel store suite: attributed baseline 16 → 1; `exit=0 samples=1631 max_concurrent_ddl=1 samples_with_ddl=1332 samples_over_1=0 foreign_ddl_max=16`
- PR #95 exact-head CI gate passed in 4m53s.
- Flowspace live indexing found the new no-FORCE guard and amended AC-0003 directly from this worktree.
- Commit attribution: `harness commit` reported `direct-verified`; `refs/notes/ai` landed for `09509b7`.

## Deviations and friction

- The restart handoff initially named two plan-010 review files and misplaced the plan-012 review ledger; o-prime corrected and supplied the ledger before coding.
- `pij whoami --json` is unsupported for rs; plain `pij whoami` verified the canary.
- Rust-analyzer returned a false-empty reference set for a known exported store symbol; exact identifier search plus anchored reads supplied caller coverage.
- `flowspace3 status` timed out while doctor and scoped semantic search remained healthy; the successful search proved the current worktree was indexed.
- The first copied DDL probe was the obsolete unattributed :5433 version. O-prime replaced it with the guarded attributed :5434 probe before execution.
- Harness observations were listed, never cleared; the shared buffer remains o-prime-owned.

## Assumptions

- The serialization guarantee is process-wide, not machine-wide. Separate seats/processes can still issue concurrent DDL; row 126 is reduced rather than closed.
- `FS3_DB_MUTATION_CONCURRENCY` is fixed before the process initializes its first database-mutation permit.
- All repository test helpers issue database DDL through the exported store primitives after this clean cutover.
- Production orphan listing/drop remains o-prime/Jordan-GO owned. This restarted seat used only `flowspace3-db-test` on :5434 and never :5433 or :7373.
