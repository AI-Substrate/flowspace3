# fresh-db-serialise restart canary + remaining-work plan

## CANARY

- pij id: `pij-little-junglefowl`
- transport: `rs`
- spawnId: `s1788317409586-86784`
- model: `github-copilot/gpt-5.6-sol-fast-1m`
- cwd: `/Users/jordanknight/substrate/flowspace/fs3-fresh-db-serialise`
- branch: `012-fresh-db-serialise`
- inherited HEAD: `f3aec31`
- test database: `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`; container `flowspace3-db-test`
- **CANARY-OK**: plain `pij whoami` resolved this restarted seat and delivery to `pij-binding-magpie` succeeded.

## Handoff integrity

Read the coder packet, replies 001–014, and `review-012-verdict.md`. Replies 002/005/009 arrived after the first read attempts and were then read. Two required files are still absent from this worktree despite the prime's completeness message:

- `.harness/temp/agent/review-012-foldin-criteria.md`
- `.harness/temp/agent/review-012-URGENT-retract-trap.md`

No code will change until those files arrive, are read in the requested order, and reply 015 clears this restart plan. The available reply 010 names four residuals, while the restart brief says six; I will not infer the missing two.

## Numbered plan for remaining work

1. Read the two missing review files, then `fresh-db-serialise-prime-reply-015.md`; reconcile every superseding ruling before touching source.
2. Run the required harness/flowspace orientation and boot receipts in this worktree; use only the :5434 test postmaster.
3. Replace the two remaining raw `DROP DATABASE ... WITH (FORCE)` bypasses in daemon support and `pg_first_light` with `fs3_store::drop_database`; verify every caller of the exported store primitives through LSP before modification.
4. Widen orphan-name parsing and tests to cover both leaked layouts named in reply 010: `fs3_storelock_<32hex>_<worker>` and `fs3_migrations_<32hex>`, without admitting unrelated databases.
5. Replace substring-based advice classification with the binding SQLSTATE-class rule: database errors outside class 57 are permanent credential/permission/configuration failures; preserve recovery advice for accept-then-close errors without SQLSTATE.
6. Move the 25 ms delay and in-flight instrumentation out of the production `create_database` body behind a test-only seam; run the architecture-drift gate for the promoted `tokio` dependency and stop if its allowlist rejects the change.
7. Apply the AC edits from reply 011: retain the attributed server-side `pg_stat_activity` receipt in ac-0001, update ac-0004 to :5434/`flowspace3-db-test`, and remove the obsolete row-124b non-goal.
8. Restore the independent-runtime regression from reply 012 at the store primitive: OS threads, one current-thread Tokio runtime each, barrier-synchronised, while retaining the N=8 real-create-path test.
9. Run focused tests and mutation checks for each changed contract, one Cargo invocation at a time, against `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test` only.
10. Run `.harness/temp/agent/ac-0001-ddl-probe.sh --check`, then the attributed store-suite probe with `CONTAINER=flowspace3-db-test`; require server-observed max concurrent attributed CREATE/DROP DDL to move from the grounded baseline 16 to `<= 1` at the default concurrency.
11. Ask `pij-binding-magpie` for the exclusive gate slot; only after CLEAR run `harness checks`, preserving the one-full-gate-at-a-time rule.
12. Commit with `harness commit`, push the branch/PR update as already authorised by the packet, update the done report with the SHA, evidence, assumptions, architecture rationale, and DDL-probe receipt, then send the pointer to `pij-binding-magpie` and hold for delta review.
