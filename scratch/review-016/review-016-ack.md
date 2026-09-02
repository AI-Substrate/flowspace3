# review-016 ack — cross-model reviewer (Claude), plan 016-hidden-dirs

- **Seat**: pij-soviet-knobbler (reviewer)
- **Worktree**: /Users/jordanknight/substrate/flowspace/fs3-review-016
- **SHA under review**: f9b6d0780cd9208a844c129748cccd45beda1d8d (branch 016-hidden-dirs, PR #107), detached — confirmed `git log -1` = `f9b6d07 feat: add per-root hidden directory indexing`
- **Base**: 82f60eceb3c955d5897151cf732e18625c14a9d3 (merge-base with main); 47 files, +2529/-79
- **Packet**: docs/plans/016-hidden-dirs/packet-reviewer.dd.md — READ IN FULL (incl. untruncated .dd.json text for i6 / owed-1)
- **Three owed lists**: PRESENT (owed-1-least-confident, owed-2-disbelieve, owed-3-known-open). Accepting the packet.
- **Packet defect noted, not a blocker**: instructions `i6` and `i7` are stale copy-paste from plan 014 (migration 0023, the job-row purge, `status --history` live-only). They do not describe 016. I am reviewing against `owed-1/2/3` + the plan ACs, and will report the stale instructions as friction rather than spend findings on them.
- **Fence honoured**: read-only on code; writes only to `.harness/temp/agent/` and `docs/plans/016-hidden-dirs/assets/reviews/`.
- **DB fence honoured**: every test run pinned to `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test` (container `flowspace3-db-test`). Any scratch daemon gets `FS3_CONFIG_DIR=<mktemp -d>` + `:5434` + a free ephemeral port. NEVER default config, NEVER `:7373`/`:5433` (row 169 — a default-config daemon clobbers prod's key). No `harness checks` (exclusive slot is not mine); targeted `cargo test` only.

## Numbered plan

### 0. Orientation (before any AC)
0.1 Read plan.dd.md, impl-guide.dd.md, assets/backpressure.dd.md, assets/inputs/evidence.md, assets/tasks/phase-1/{tasks,execution.log}.
0.2 Read the author receipts `.harness/temp/agent/hidden-dirs-t*-complete.md`, the asks, and `hidden-dirs-pr-body.md` — as CLAIMS to be re-derived, per owed-2. No receipt is evidence until I reproduce it.
0.3 Read the **full diff** base..HEAD for the 29 code/test files (docs files skimmed).
0.4 Read PR #107 CI status on the exact sha (`pr://107`) — CI runs on the PR, per owed-3.
0.5 Dogfood: use `flowspace3 search` (read-only, prod index) to find consumers of `DiscoverySettings`, `include_hidden`, `RootReport`, the ledger reason enum, and the two call sites — record any miss as friction via `harness observe`.

### 1. AC-0001 — flag persisted per root; envelope reflects it; absent re-add preserves, explicit `--no-include-hidden` clears
1.1 Read `crates/store/migrations/0024_worktree_include_hidden.sql` — assert it is a single additive `ALTER TABLE worktrees ADD COLUMN include_hidden BOOLEAN NOT NULL DEFAULT FALSE` shape (risk-3: instant, no table rewrite, no backfill scan), and that the migration list/checksum plumbing in `crates/store/src/lib.rs` registers it in order.
1.2 owed-1(f): prove 0024 is additive and idempotent-by-ledger — run the store migration suite twice against a DB that already has 0024 applied (`migrating_twice_changes_nothing` or its equivalent) and read the exit status myself.
1.3 Read `crates/store/src/refs.rs` (worktree upsert) for the `Option<bool>` semantics: assert the SQL uses COALESCE-on-absent (or an explicit conditional) so `None` = preserve stored value and `Some(false)` = write false. Check the UPSERT's `ON CONFLICT DO UPDATE` set-list explicitly — this is where a silent reset lives.
1.4 Read `crates/daemon/src/http.rs` POST /roots body + `crates/daemon/src/roots.rs` add_root + `crates/core/src/views/roots.rs` RootReport: assert `include_hidden: Option<bool>` survives serde with `#[serde(default)]` and that an OLD client body (no key) deserialises to `None`, not `Some(false)`.
1.5 Read `crates/cli/src/main.rs` + `client.rs`: assert clap gives three states (flag → `Some(true)`, `--no-include-hidden` → `Some(false)`, neither → `None`) — i.e. it is NOT a bare `bool` with `default_value=false`, which would silently reset on every plain re-add.
1.6 Run the targeted suites myself on :5434: `cargo test -p fs3-store`, `-p fs3-daemon` (roots/first_light/worktree_lifecycle), `-p fs3-cli`, `-p fs3-core` — read exit status, not summary prose.
1.7 **My own mutation (not the author's)**: change the upsert so absent (`None`) writes `false` (drop the preserve branch) and assert a designated test goes RED; restore and confirm GREEN. Author's mutation was "forced-false store mutation" — mine targets the *preserve* branch specifically, which is the one that can rot silently.
1.8 **End-to-end through the CLI**, not just unit level (owed-1(b)): scratch daemon, `add --include-hidden`, plain re-`add`, re-read persisted value; then `add --no-include-hidden`, re-read. Three transitions, observed from the envelope AND from the DB row.

### 2. AC-0002 — both discovery call sites honour the flag; `.git` never; deny list survives
2.1 Read `crates/daemon/src/roots.rs` around the old :178 construction: assert `include_hidden = config.scan.include_hidden || worktree.include_hidden` (OR, not override) and that it is computed per-root, not once per daemon.
2.2 Read `crates/daemon/src/watch.rs` around the old :275: **the one people forget**. Assert the watcher's `DiscoverySettings` is rebuilt per rescan from the freshly-read worktree row, not captured once at watcher startup (risk-1). If it is captured in a struct field at spawn, that is a defect regardless of what the test shows.
2.3 owed-1(a): prove the **live** flip — a `add --include-hidden` (and the reverse) reaching a **running** watcher's next rescan with **no daemon restart**, using the real watcher test (`crates/daemon/tests/watcher.rs`), not a unit stub. Read the test to confirm it drives a live watcher; if it stubs, I re-derive with a scratch daemon by hand.
2.4 owed-1(c): deny-list mutation check. Assert `.git` is refused **regardless** of include_hidden, and that `node_modules` / `.venv` / `.cache` are pruned **even when nested inside a hidden directory** (e.g. `.hidden/node_modules/x.ts`) — read `crates/parsers/src/discovery.rs` prune logic for whether the deny check runs before/independently of the hidden check, and add my own fixture case if the existing fixtures only test top-level.
2.5 **My own mutations**: (i) drop the per-root OR in `watch.rs` only → assert the watcher test goes RED (proves the watcher test actually covers the watcher path and is not passing via the roots path); (ii) remove `.git` from the deny list while include_hidden is true → assert a designated test goes RED. Restore both, confirm GREEN.
2.6 Confirm the initial scan, the re-add, AND the watcher rescan are three distinct proven paths — not one path proven three times.

### 3. AC-0003 — ledger `hidden=N` reason; status/tree expose include_hidden per root
3.1 Read `crates/parsers/src/discovery.rs` ledger: assert `hidden` is a real counted reason incremented at the actual prune site, and is not conflated with the generic ignore reason.
3.2 owed-1(d) **measured, not decorative**: mutation — make the hidden prune site increment a *different* reason (or drop the increment) and assert the designated daemon test goes RED with a count mismatch, not merely an absent key. Then a value mutation: assert the test would catch `hidden=1` vs `hidden=2` (i.e. it asserts the count, not `>0`).
3.3 owed-1(e) **EVERY root row, not only cwd**: read `crates/daemon/src/status.rs` + `crates/core/src/views/status.rs` + `crates/daemon/src/read.rs` (tree) + `crates/core/src/views/read.rs`. Then run a scratch daemon with **two** roots — one opted in, one not — and assert `status --json` and `tree --json` report the correct per-row `include_hidden` for BOTH, including when cwd is outside both roots. The author's receipt says "cwd/absolute tree data includes resolved root policy" — that phrasing is exactly where a cwd-only implementation hides.
3.4 Envelope compatibility (the spirit of i7, applied to 016): assert `include_hidden` is **added**, no existing key renamed or removed in `RootReport` / status / tree JSON. Diff the envelope keys before/after by running the base binary vs the branch binary against the same scratch DB, or by reading the view structs field-by-field.
3.5 Check `add` human output actually explains the omission — the plan promises the ledger "explains what a default add left out". Judge the human (non-JSON) surface too, and the `--help` text for the new flags.

### 4. AC-0004 — no regression
4.1 Targeted only (no `harness checks` — not my slot): `cargo test -p fs3-parsers -p fs3-daemon -p fs3-store -p fs3-cli -p fs3-core` on :5434, plus `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check`. Read exit codes.
4.2 Read PR #107 CI conclusion **on sha f9b6d07** specifically — a green run on an earlier sha is not evidence (ruling 2026-09-02-ci-green-on-the-sha-is-the-gate).
4.3 Inspect the test-file diffs for **weakened** assertions: `crates/daemon/tests/{event_stream,lanes,remove_root,worktree_lifecycle}.rs` and `crates/parsers/tests/discovery_*.rs` changed. A regression hidden as a "test update" is the classic. Every changed assertion must be justified by the new column/flag, not by making a failure go away.
4.4 The author's own AC-0004 receipt cites a "known-open isolated real-daemon health timeout". I will confirm o-prime ruled it non-016 and spend zero findings there (owed-3), but I will check it is genuinely pre-existing on base 82f60ec if cheap.

### 5. AC-0005 — real-usage receipt reproducible on a TEST daemon
5.1 owed-1(g)/owed-2: re-run the PR-body transcript **myself**. Scratch: `FS3_CONFIG_DIR=$(mktemp -d)`, DB `:5434`, a free ephemeral port I pick. Never the prod key file, never `:7373`/`:5433`.
5.2 Baseline count from git, not from the receipt: `git -C ~/pi-hacking/pij ls-files '.pi/**/*.ts' | wc -l` — compare with the claimed 379 tracked. If today's number differs, the receipt is stale, and I judge the *shape* (all tracked TS evaluated; exactly the NUL-byte file excluded) rather than the literal integer.
5.3 Default add first: assert `.pi/` rows = 0 and the scoped `.pi/**` search returns `path_unmatched`.
5.4 Opt-in add: assert stored = tracked − binaries; independently verify `.pi/extensions/pij/core/memorable-id.test.ts` really contains NUL in its first 8 KiB (`head -c 8192 | tr -d '\0' | wc -c` delta) rather than trusting "3 NUL bytes"; assert `binary=1` in the add ledger.
5.5 Search `export function daemonLocation(` and assert the returned element resolves to a `.pi/extensions/pij` path; `get` the element id. Cite what I saw, not what the receipt says.
5.6 Kill the scratch daemon and remove its config dir. Verify prod (`:7373`) was never touched — confirm prod's key file mtime is unchanged across my run.

### 6. Seams, drift, and safety (i3 priority 2–4)
6.1 Composition seams (single unit, but four crates): store column → core view → daemon handler → CLI renderer. Check each hop for the `Option<bool>` collapsing to `bool` too early — the classic drift.
6.2 Interface drift vs impl-guide u1: CLI `add --include-hidden|--no-include-hidden`; POST /roots `include_hidden: Option<bool>`; `RootReport` + status/tree JSON `include_hidden: bool`; store `worktrees.include_hidden`; ledger reason `hidden`. Any deviation reported.
6.3 `remove_root`, `worktree_lifecycle`, `event_stream`, `lanes` touched — confirm the new column does not break root removal / re-add identity or the event contract.
6.4 Unrelated files in the diff (`assets/inputs/db-cpu-profile-report.md`, `pij-two-daemons.md`) — note as scope, not a code finding.

### 7. Deliverables
7.1 `docs/plans/016-hidden-dirs/assets/reviews/review-016-<n>.dd.json` + `ddocs build` → `.dd.md`.
7.2 `ddocs validate` (GLOBAL ddocs, run from the worktree root) on the review record — **build is not validate**; severities MAJOR/MINOR/NIT/NA, kinds defect/dim0/question, ids `<prefix>-<4 hex>`. Not "built" until validate is clean.
7.3 `.harness/temp/agent/review-016-verdict.md` with a one-line verdict + every AC judged true/false with cited evidence (done-bar d1), seams reported (d2).
7.4 `pij send pij-binding-magpie '<path> + one-line verdict'`, mirrored into `.harness/temp/agent/review-016-status.md`.
7.5 Friction captured with `harness observe` as it happens (flowspace search misses, stale packet instructions, anything I had to infer twice). I will **list and report** observations; I will NOT drain or `--clear` the shared buffer (worker rule, 2026-08-26).

## Gate
Holding for the o-prime ruling before executing sections 1–7, per instruction.
