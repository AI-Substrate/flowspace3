# fresh-db-serialise coder acknowledgement

- pij id: `pij-mad-crocodile`
- spawnId: `s1788309142136-5995`
- model: `github-copilot/gpt-5.6-sol-fast-1m`
- cwd: `/Users/jordanknight/substrate/flowspace/fs3-fresh-db-serialise`
- branch: `012-fresh-db-serialise`
- `CANARY-OK`

## Numbered plan

1. Wait for `fresh-db-serialise-prime-reply-001.md`; make no source changes before the ruling. After GO, run Serena's coding instructions, inspect the owned helper through Flowspace first, and use rust-analyzer references before changing any exported symbol. Keep every file operation rooted at this worktree.
2. `tk-0101`: characterize concurrent database creation, then add a process-wide async semaphore around every `fs3_store::create_database` and `drop_database` call in `FreshDatabase`. Size it from `FS3_TEST_DB_CONCURRENCY`, default 1; document the invariant. Prove at most N operations in flight and record the remove-permit mutation that makes the test fail.
3. `tk-0102`: add refused-port and listening-then-closing/recovery advice tests, then classify the store/connect error so only a genuinely absent server recommends starting one. A server that answered must produce wait-and-retry recovery wording and no compose suggestion.
4. `tk-0103`: factor one strict parser for both minted tails, `fs3_test_<epoch>_<32hex>` and `fs3_<label>_<epoch>_<32hex>`, preserving the age threshold and rejecting prod-like names. Add list-only candidate discovery, reuse it for destructive sweep, and prove two old names are selected while fresh and non-test names remain.
5. After each implementation task, update the deterministic task state via `dd`, rebuild its generated view through that command, and append exact evidence to the execution log; do not hand-edit generated `.dd.md` files or widen beyond `crates/testkit` without a stop-and-ask file.
6. `tk-0104`: on the shared compose database only, run the daemon oversize suite at default parallelism with a stated log window and assert zero postmaster recovery signatures. Any red tripwire stops the packet and is reported rather than investigated around. Never target prod `:7373`.
7. `tk-0105`: run focused tests, then `harness checks`; cite tree-resident evidence, commit only through `harness commit`, open a PR into `main`, state the lock-removal mutation, and report CI evidence. Do not merge.
8. `tk-0106`: provide the list-only command/interface and assumptions to o-prime. The production orphan listing, GO, drop, and before/after counts remain o-prime-owned.

## Preflight evidence and constraints

- `harness --version`: 0.14.0.
- `harness doctor --json`: degraded because transient harness scratch is unprotected; six extensions loaded.
- `harness boot --json`: build passed; degraded because compose service `db` is not running. I did not start it before the ruling.
- rust-analyzer: configured; not started until the first LSP request.
- Flowspace front doors (`agents-start-here`, `docs list`) returned valid JSON envelopes.
- This rs-resident OMP conversation is not ingestible under the current pij split; file channel only, as ruled.
