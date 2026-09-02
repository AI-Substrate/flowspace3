# hidden-dirs DONE report

## Delivery

PR: https://github.com/AI-Substrate/flowspace3/pull/107

Current pushed head: `fa4da2fe8b341c6a0ea320ffae262d3c72075de7`

Commits:

- `f9b6d0780cd9208a844c129748cccd45beda1d8d` — `feat: add per-root hidden directory indexing`
- `fa4da2fe8b341c6a0ea320ffae262d3c72075de7` — `docs: record hidden directory usage receipt`

O-prime saw `f9b6d07`; the second commit is documentation-only and records PR #107 in the T5 task/execution log. It was pushed before the HOLD instruction arrived. No push occurred after HOLD.

Shipped:

- migration `0024_worktree_include_hidden.sql`: `BOOLEAN NOT NULL DEFAULT FALSE`; 0023 untouched
- per-worktree stored policy; absent add preserves stored state, explicit true/false update it
- CLI `add --include-hidden` / `--no-include-hidden`; additive `/roots`, `RootReport`, status, and tree fields
- initial/re-add discovery and watcher subtree discovery resolve `global OR per-root`; watcher re-reads current stored policy per relist
- hidden directory prunes are named once and aggregated as `hidden=N` without descending; `.git` and standard denies remain enforced
- status and concrete-root tree output expose `include_hidden` in JSON and human views

## Acceptance evidence

### AC-0001

Store, daemon, CLI parser, and core round-trip tests pass. True → omitted re-add → true → explicit false is proven. Forced-false column-write mutation failed at the expected persisted-true assertion; restoration passed.

### AC-0002

Default fixture maps only `src/b.ts`; opt-in also maps `.hidden/a.ts`; `.git`, `node_modules`, and `.venv` never map. Live watcher indexes a hidden file while enabled, then rejects a new hidden file after explicit-false re-add without supervisor restart. Independent root/watch call-site mutations each turned the designated test red; restorations passed.

### AC-0003

Parser fixture/standard-ignore/subtree suites pass. Default add reports `hidden=2` for `.hidden` and `.venv`; status root carries true; cwd-scoped tree carries true; CLI renderers show the policy. Removing hidden aggregation made the designated daemon test red; restoration passed.

### AC-0004

Exclusive `harness checks` completed `status=ok` at `2026-09-02T07:07:06Z`: docs, lock, test DB, harness contracts, fmt, clippy, and all tests green. The later ruled isolated real-daemon health probe timed out; exact output is `.harness/temp/agent/health-isolated.log`. O-prime classified this as known-open plan-017 environment evidence, not a 016 regression, because boot/auth/health are untouched and the test passed inside the full gate.

### AC-0005

Scratch runtime only:

- config: `/tmp/fs3-hidden-dirs.gis4B3/config.toml`
- DB: `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`
- daemon: `http://127.0.0.1:63624`
- default config, production key, `:7373`, and `:5433`: untouched

Default explicit false: `include_hidden=false`, `hidden=14`, `.pi/%` rows `0`; repo/path-scoped `.pi/**` search returned `path_unmatched`.

Opt-in: `include_hidden=true`, `489` newly discovered files. Current corpus has `379` git-tracked `.pi/**/*.ts`; `378` indexable paths stored. Exact exclusion `.pi/extensions/pij/core/memorable-id.test.ts` is `data` with three NUL bytes in its first 8 KiB, matching add's `binary=1`. Search for `daemonLocation` returned exact-name score `1.0` at `el:git:github.com/AI-Substrate/pij/.pi/extensions/pij/adapters/daemon-http.ts::daemonLocation`, span `62-72`; `get` returned the function body. Scratch daemon stopped cleanly (`exit=0`).

O-prime amended AC-0005/BP-0005 from the stale untracked-inclusive `>=500` threshold to this tracked-corpus invariant. Plan, task, assertion, and backpressure documents were mutated only through global `ddocs`.

`harness plan validate ...tasks.dd.json#tasks`: `error=0`, `warn=0`, `open=0`, `contradictions=0`, `orphans=0`.

## Deviations / noteworthy

- Packet named nonexistent `crates/store/src/worktrees.rs`; o-prime amended the fence to LSP-resolved store/core/daemon owners.
- Ddoc task state has no `in-progress` value; start boundaries stayed in agent reports, then task/assertion rows became `checked` after proof.
- T5 inherited count was corrected to current tracked corpus by o-prime ruling.
- CI on current PR head was in progress when HOLD arrived; watch was cancelled. Local full gate is green.

## Assumptions

- Migration 0024 runs before the new daemon handles root/status/tree requests.
- `RootRequest.include_hidden=None` means preserve existing per-root state; a new root defaults false through the schema.
- Global `scan.include_hidden=true` may make discovery effective-true while the stored per-root field remains false; surfaced fields intentionally report the per-root choice.
- Tree reports `include_hidden` only when cwd or an absolute target resolves one concrete worktree; index/repo-only views with no concrete root omit it. Status is the authoritative all-roots listing.
- A watcher relist always has a registered worktree id; a missing row safely resolves hidden false.
- O-prime owns production migration/bounce and post-merge production receipt.

## Composer / reviewer notes

Single unit; no composition wiring remains. Review should focus on absent-add preservation, watcher live-policy refresh, manual hidden prune equivalence to the prior walker filter, and additive tree/status compatibility.

## Harness observation buffer

Listed with `harness observe --list`; NOT cleared. Captures include degraded boot/doctor signals, stale Flowspace/DDoc messages, packet fence misses, missing PATH `psql`, ddoc state-vocabulary mismatch, read-only prod search outage, real-daemon health timeout, stale T5 threshold, and shared-test-DB queue latency. O-prime owns drain/clear.
