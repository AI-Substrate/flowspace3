# review-016 DELTA VERDICT — `fa4da2f..aa1abb6`

**VERDICT: BOTH MEDIUMs CLOSED. NO NEW DEFECT ENTERED. Ship it.**

- **Delta sha:** `aa1abb61d9d427a1138fd8ce3622834d5b4468b1` — committed, pushed, PR #107 head, reviewed **detached in my own worktree** (`git log -1` → `aa1abb6 fix: correct hidden-directory policy reporting`). DL-011 held.
- **Scope:** `fa4da2f..aa1abb6` only — 10 files, **+642/−76**, matching the handoff exactly. Nothing outside it examined or judged.
- **The three MINORs (f-16b1/b2/b3, backlog 174/175/176) are OUT OF SCOPE** and are not re-litigated here. One incidental note on f-16b1 appears below only because the delta moved its number; it is not a finding and needs no action.
- **My record from round 1 is committed verbatim** — `diff -q` against my local copy before checkout: **IDENTICAL**.

## Head drift `aa1abb6..ca9d255` — docs-only, VERIFIED MYSELF

```
$ git log --oneline -1 ca9d2556ae842e6a273fb5cda2302e081f0e7136
ca9d255 docs: record review delta full gate

$ git diff --stat aa1abb6 ca9d255
 docs/plans/016-hidden-dirs/assets/tasks/phase-1/execution.log.md | 4 ++++
 1 file changed, 4 insertions(+)

$ git diff --name-only aa1abb6 ca9d255 -- crates/ | wc -l
0

$ git diff --name-only aa1abb6 ca9d255 -- crates/ Cargo.lock Cargo.toml .github/ | wc -l
0
```

Confirmed independently, not taken on trust: **one** file, **+4/−0**, and zero paths under `crates/` — widened by me to also cover `Cargo.lock`, `Cargo.toml` and `.github/`, still zero. The four added lines record an exclusive `harness checks` on `aa1abb6` completing `status=ok` at `2026-09-02T08:20:07.988Z`. **This delta review of `aa1abb6` therefore stands unchanged and covers PR head `ca9d255`.**

**Merge gate met on the new head:** run `33608236344` on `ca9d2556ae842e6a273fb5cda2302e081f0e7136` → `status=completed conclusion=success`; PR #107 rollup `gate = SUCCESS`, head `ca9d255`, `mergeable`. CI is now green on **all four** shas of this branch — `f9b6d07`, `fa4da2f`, `aa1abb6`, `ca9d255`.


## Gate: CI green on the sha

```
run 33607438561  name=ci  head_sha=aa1abb61d9d427a1138fd8ce3622834d5b4468b1
                 status=completed  conclusion=success
PR #107 rollup:  gate = SUCCESS      head = aa1abb6      mergeable = MERGEABLE
```

Per `2026-09-02-ci-green-on-the-sha-is-the-gate.md` this is the gate and it is met **on the delta sha itself**. Locally I also ran `cargo fmt --all --check` → FMT_OK and `cargo clippy --all-targets -- -D warnings` → **exit 0, zero warnings**. That the author ran fmt + targeted suites rather than the full slot is accepted, not a finding.

## f-16a1 — CLOSED

**Fix as landed:** the deny-list branch moved ahead of the hidden branch and is gated `is_dir &&` (so the old `!is_dir → true` early-return is preserved), and the fix string moved onto `PruneReason::fix()` so reason and advice can no longer drift apart.

**My gate, run black-box on a scratch daemon (`:54980`, DB `fs3_rev016d`), fixture with `.venv`, `.cache`, `.next`, a plain `.hidden`, `.hidden/node_modules`, `.git`, `node_modules`:**

| directory | default mode | opt-in mode | verdict |
| --- | --- | --- | --- |
| `.venv` / `.cache` / `.next` | `standard-ignore` + `standard_ignores = false` | `standard-ignore` + `standard_ignores = false` | **reason no longer flips**; fix correct in both |
| `.hidden` | `hidden` + `--include-hidden` | admitted (`files` 1→2, `.hidden/a.ts` mapped) | **the advertised fix now works** |
| `.hidden/node_modules` | — | `standard-ignore` | my round-1 extra case still holds |
| `.git` | never mapped | never mapped | unconditional refusal intact |

The exact failure I reported is gone: the ledger no longer advertises `--include-hidden` on any row where `--include-hidden` would not change the outcome.

## f-16a2 — CLOSED

**Fix as landed:** `file_tree` resolves the policy from the **selected** `IndexedFile.root_path` via `find_worktree`, and `tree_hidden_policy` returns **absent** for explicit non-absolute targets instead of borrowing `scope.worktree`.

**My gate — root A `include_hidden=true`, root B `include_hidden=false`:**

| case | round 1 (`f9b6d07`) | delta (`aa1abb6`) |
| --- | --- | --- |
| cwd=A, `tree el:<B>/src/x.ts` | `true` ← **the defect** | **`false`** ✓ |
| cwd=B, `tree el:<A>/src/x.ts` | `false` ← mirror | **`true`** ✓ |
| human title, cwd=A → B's file | `· hidden yes` | **`· hidden no`** ✓ |

**All three controls survive the fix** — it did not buy the wrong-answer case by breaking a right-answer one:

- plain `tree` inside A → `true`, inside B → `false`
- `tree <absolute path of B>` from A → `false`; the reverse → `true`; absolute **file** target → `false`
- `tree --repo <B>` from A → `null` (honest absence, never A's value)
- index view → `null`

**Accepted trade, named for the record, not a finding:** `tree <relative dir>` inside your own root now returns `null` where it previously returned that root's *correct* policy (control C4). The author chose the blunter rule — any explicit non-absolute target is absent — over my narrower "absent only when the resolved repo differs from cwd's". It produces **no wrong answer anywhere**, honest absence is the shape this function already uses for the index and conversation views, and `status --json` remains the authoritative all-roots listing carrying the flag per root, so ac-0003 is unaffected. Strictly safer than what I flagged.

## No new defect entered

Beyond the two gates I hunted the reorder's blast radius:

- **Files named like deny entries** — the old `if !is_dir { return true; }` guard sat *before* the deny check and is now expressed as `is_dir &&`. Probed: `target.ts`, `dist.ts`, `src/vendor.ts` all indexed; only the `build` **directory** pruned. A hidden file named `.venv` is skipped without being mislabelled a directory prune.
- **Force-include pass** — the pass runs with an empty deny list, so the reorder could have relabelled ledger rows. `force_include_reaches_into_a_denied_directory` and `force_include_reaches_into_a_gitignored_folder` both pass, and my live default add reported `standard-ignore` for `.venv`, so the two passes' `or_insert` ordering still lands on the deny-list reason.
- **`.git` at any setting** — `git_internals_are_unwalkable_at_any_setting` passes; `.git/never.ts` never mapped in either mode live.
- **Prune ledger shape** — `every_denied_directory_is_named_in_the_prune_ledger`, `the_prune_ledger_never_descends`, `an_empty_deny_list_prunes_nothing_and_reports_nothing` all pass.
- **Both discovery call sites still honour the per-root flag** — full `fs3-daemon --test watcher` suite green (11 passed), including the live-watcher runtime flip.

## The new tests are load-bearing — proven without a mutation

The packet fence keeps me read-only on code, so I did not re-run the author's mutations. I did not need to: I **directly observed the pre-delta behaviour at `f9b6d07`** in round 1, and the new assertions contradict it.

- `hidden_directory_prunes_name_the_effective_rule` now asserts `.venv → "standard-ignore"` under `include_hidden=false`. At `f9b6d07` I observed `.venv → "hidden"` in that mode. The test **must** have failed before the fix.
- `tree_policy_tracks_the_resolved_root_in_both_directions` now asserts B's file → `false` with cwd=A. At `f9b6d07` I observed `true`. Same conclusion.

Both are therefore genuine regression locks, not assertions written to match whatever the code now does. The parser test goes further than I asked and **executes the fix chain to completion** — `standard_ignores = []` reclassifies `.venv` to `hidden` with the `--include-hidden` advice, and enabling both admits it — so every fix string in the ledger now terminates in an actually-indexed directory.

## Suites

`cargo test -p fs3-store -p fs3-daemon -p fs3-parsers -p fs3-cli -p fs3-core` on `:5434` — everything green **except** `tests/health.rs::the_real_binaries_agree_through_a_discovered_config`, which is the already-adjudicated **f-16c1**: the shared `flowspace3_test` database still holds **19** registered roots and boot's pre-serve ddoc probe exceeds the test's 10 s ceiling. Same failure, same message, same cause as round 1; unrelated to this delta. Named tests re-run individually: `tree_policy_tracks_the_resolved_root_in_both_directions` ok · `hidden_directory_prunes_name_the_effective_rule` ok · `discovery_standard_ignores` 14/14 · `discovery_fixtures` 10/10 · `watcher` 11/11 · `hidden_files_are_discovered_only_for_an_opted_in_root` ok · `add_hidden_policy_round_trips_and_absence_does_not_reset_it` ok · `tree_title_agrees_with_the_resolved_root_policy` ok · `roots_show_their_hidden_directory_policy` ok.

*Incidental, no action:* the reorder drops the `skipped` `hidden` count on my fixture from 3 to 1, because deny-listed dot-names no longer land in it. That narrows backlog **174 (f-16b1)** without closing it — the count is still directories among per-file counts. Recorded so 174 is re-measured against `aa1abb6`, not against `f9b6d07`.

## f-16c1 — the drift commit CLOSES the diagnosis

The receipt `ca9d255` records reports a **full `harness checks`** on `aa1abb6` passing the whole test suite — including the very health test that fails for me. That is not a contradiction; it is the last piece of the mechanism, and it upgrades f-16c1 from "environmental" to fully explained.

`.harness/extensions/checks/instructions.md` gate 7 (`fs3-test-suite`) "mints and migrates a unique `fs3_test_<epoch>_<entropy>` child with `FreshDatabase`, injects **only that URL** into `cargo test --all`, then force-drops it."

So the gate hands the suite a **freshly migrated, empty** database — **0 registered roots → boot in ~1 s → the health test passes**. Running the same binary with `FS3_TEST_DATABASE_URL` pointed straight at the shared `flowspace3_test` hands it **19 roots → key after 25 s → the 10 s ceiling is missed**. The variable is *which database the test binary is handed*, nothing else.

This retro-explains the author's round-1 chronology exactly (ask-006: `harness checks` green at 07:07:06Z, then the mandated isolated probe red minutes later) — it was never load, timing or flakiness — and it explains why CI is always green. For the 19-root product row: the honest framing is that the pre-serve ddoc probe is unbounded in root count **and** that the failure is invisible to every gate we run, because every gate runs against an empty database. A test that only ever sees a fresh store cannot observe a cost that scales with registered roots.


## Fence

Read-only on code; **no source mutations**. Scratch daemon `:54980` with `FS3_CONFIG_DIR=/tmp/fs3-rev016d` against a database I created and dropped (`fs3_rev016d`). **`flowspace3_test` untouched — still exactly 19 roots, before and after** (cod's gate safe). Prod never touched: no `:7373`, no `:5433`, default daemon key mtime unchanged at `16:57:45`, i.e. hours before any daemon I started. No `harness checks`. All scratch daemons stopped, scratch DBs dropped, temp fixtures removed. Worktree left with only o-prime's own uncommitted `packet-reviewer.dd.{json,md}`.
