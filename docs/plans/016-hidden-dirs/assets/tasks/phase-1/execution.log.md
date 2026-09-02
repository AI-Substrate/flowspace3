# Phase 1 execution log

## T1 — Persist the per-root opt-in

Status: complete.

Implemented migration `0024_worktree_include_hidden.sql`; store read/write support; additive `RootRequest.include_hidden: Option<bool>` and `RootReport.include_hidden`; CLI `add --include-hidden` / `--no-include-hidden`; absent re-add preserves stored state.

Evidence:

- Dedicated database probe: `select 1` through `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test` returned `1`.
- `cargo test -p fs3-store --test pg_first_light a_worktree_hidden_policy_survives_reregistration_until_explicitly_changed -- --exact`: PASS.
- `cargo test -p fs3-daemon --test first_light add_hidden_policy_round_trips_and_absence_does_not_reset_it -- --exact`: PASS.
- `cargo test -p fs3-cli add_hidden_flags_are_explicit_and_mutually_exclusive -- --exact`: PASS.
- `cargo test -p fs3-core every_view_reads_back_what_it_writes -- --exact`: PASS.
- Mutation receipt: changed the store setter to bind `false`; the store test failed at the expected persisted-true assertion (`pg_first_light.rs:173`); restored the real bind and the test passed.

Discoveries:

- Noteworthy: the packet's store path did not exist; o-prime amended the fence to the LSP-resolved owners and additive public-view files.
- Noteworthy: temp paths canonicalize to `/private/...` on macOS, so the daemon test verifies persistence against the returned `root_path`, not the caller's pre-canonical path.
- Harness: task state has no `in-progress` vocabulary; live start is represented by the boundary report while the ddoc remains `unchecked`, then assertion/task become `checked` after proof.

## T2 — Thread the flag through discovery

Status: complete.

Root add/re-add resolves `include_hidden` from the explicit request or stored worktree policy, then ORs it with the global scan setting. Watcher subtree relists query the current stored policy by worktree id before every discovery call, so a live re-add takes effect without restarting the watcher.

Evidence:

- `hidden_files_are_discovered_only_for_an_opted_in_root`: PASS; default mapped only `src/b.ts`, opt-in also mapped `.hidden/a.ts`, and `.git`, `node_modules`, `.venv` stayed absent.
- `watcher_rescans_follow_the_current_per_root_hidden_policy`: PASS; watcher indexed `.hidden/live.ts` while enabled and refused `.hidden/ignored.ts` after an explicit false re-add without supervisor restart.
- Root call-site mutation (`settings.include_hidden |= false`) failed the opted-in count assertion (`1 != 2`); restored PASS.
- Watcher call-site mutation (`settings.include_hidden |= false`) failed with `the opted-in watcher skipped a hidden file`; restored PASS.

## T3 — Explain hidden skips; surface root policy

Status: complete.

Discovery now disables the opaque walker hidden filter and performs the same no-descent decision in its entry filter, recording each hidden directory once as `PruneReason::Hidden`. The add skip summary aggregates those prunes as `hidden=N`; named prune rows prescribe `add --include-hidden`. Status roots carry the stored bool. Cwd- or absolute-root-scoped tree results carry the resolved root policy; status and tree human renderers display it.

Evidence:

- Parser fixture, standard-ignore, and subtree suites: PASS. Hidden prunes are named without descending; enabling hidden reveals `.hidden` while `.git` stays refused.
- Daemon hidden fixture: PASS with `hidden=2`, status `include_hidden=true`, and permanent denies intact.
- Daemon cwd-scoped tree policy test: PASS.
- CLI status/tree hidden-policy renderer tests: PASS.
- Mutation changed hidden-prune aggregation to the wrong reason; the daemon test failed because no `hidden` row existed; restored PASS.

Discovery:

- Noteworthy: `flowspace3 search` became unavailable at read-only `:7373` during T3. The outage was captured; no production process was started or touched. Exact in-fence source reads completed the task.

## T4 — Regression and exclusive gate

Status: complete by o-prime ruling.

The initial scoped regression invocation reached the real-binary health test and timed out waiting for its free-port daemon. Under the granted exclusive slot, `harness checks` first identified formatting drift; `cargo fmt --all` repaired it. The second `harness checks` completed `status=ok` at `2026-09-02T07:07:06Z`, covering docs, lock metadata, dedicated test DB, harness contracts, formatting, clippy, and the full tests.

The subsequently ruled isolated health probe timed out again. Exact output is `.harness/temp/agent/health-isolated.log`. O-prime classified it as an environment/harness question for plan 017 because this packet does not touch boot/auth/health and the same test passed inside the full green gate. The full green gate is the accepted T4 verdict.

## T5 — TEST-daemon real-usage receipt

Status: complete.

Scratch-only runtime: `FS3_CONFIG_DIR=/tmp/fs3-hidden-dirs.gis4B3`, database `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`, daemon `http://127.0.0.1:63624`, fake providers. No production config, key, port, or database was touched.

Default receipt:

- Explicit `add /Users/jordanknight/pi-hacking/pij --no-include-hidden` returned `include_hidden=false`, `files=1974`, `hidden=14`, `unchanged=1974`.
- Database `.pi/%` worktree rows: `0`.
- Repo/path-scoped search under `.pi/**`: zero results with `empty_because.reason=path_unmatched`.

Opt-in receipt:

- `add ... --include-hidden` returned `include_hidden=true`, `files=2463`, `enqueued=489`, `unchanged=1974`, `binary=1`.
- Current tracked corpus: 379 `.pi/**/*.ts` paths; 378 stored. Exact difference: `.pi/extensions/pij/core/memorable-id.test.ts`; `file` classifies it as data and its first 8 KiB contain three NUL bytes, so the existing binary sniff correctly refuses it.
- `search "daemonLocation" --repo git:github.com/AI-Substrate/pij --path ".pi/extensions/pij/adapters/daemon-http.ts"` returned exact-name score `1.0` at `el:git:github.com/AI-Substrate/pij/.pi/extensions/pij/adapters/daemon-http.ts::daemonLocation`, span `62-72`.
- `get` on that address returned the function body from the pij worktree.
- Scratch daemon stopped cleanly.

O-prime amended AC-0005/BP-0005 from the stale untracked-inclusive `>=500` count to the tracked-corpus invariant above. The AC, backpressure row, T5 task, and assertion were changed through global `ddocs`.

## Phase complete

All five task rows and assertions are checked and receipted. `harness plan validate ...#tasks` reports `error=0`, `warn=0`, `open=0`, `contradictions=0`, `orphans=0`. Full `harness checks` passed on this worktree at `2026-09-02T07:07:06Z`.

Pull request: https://github.com/AI-Substrate/flowspace3/pull/107 (`f9b6d0780cd9208a844c129748cccd45beda1d8d` plus this receipt update).

## Review delta — f-16a1 / f-16a2

`f-16a1`: standard deny-list directories are classified before hidden policy, so `.venv`, `.cache`, and `.next` consistently report `standard-ignore` with the executable `standard_ignores = false` fix in both hidden modes. Plain `.hidden` retains `hidden` plus `--include-hidden`. The delta fixture executes the standard-ignore fix and verifies the remaining hidden-policy boundary. Mutation gated the deny-list check on `include_hidden`, recreating the review defect; `hidden_directory_prunes_name_the_effective_rule` failed with `left: hidden, right: standard-ignore`, then passed after restoration.

`f-16a2`: file tree policy now comes from the selected `IndexedFile.root_path`; unresolved repository/directory targets remain `None` rather than borrowing cwd state. The delta test registers two roots with opposite policies and proves cross-root element addresses symmetrically, plus plain-cwd, absolute-path, and explicit-other-repo controls. Renderer tests cover `hidden yes`, `hidden no`, and honest absence. Mutation restored cwd-based lookup; the cross-root test failed with `left: true, right: false`, then passed after restoration.

Targeted proof: full `discovery_standard_ignores` integration suite (14 passed), daemon hidden add-envelope test, daemon cross-root tree test, CLI tree-title test, `cargo fmt --all --check`, and `ddocs validate` on the committed review record all passed. Full harness gate intentionally not run: another seat holds the exclusive slot.
