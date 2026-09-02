# review-016 VERDICT — plan 016-hidden-dirs

**VERDICT: APPROVE WITH FINDINGS.** All five ACs judged **TRUE**. 2 MEDIUM + 3 MINOR findings, none falsifying a plan promise — every one lives in the agent-facing *explanation* of the flag, not in the flag's behaviour. Recommend merge and prod bounce; fix the two MEDIUMs in a follow-up.

- **Record (built + validated):** `docs/plans/016-hidden-dirs/assets/reviews/review-016.dd.json` → `.dd.md`
- `ddocs build` → `status: ok`. `ddocs validate` (GLOBAL ddocs, from the worktree root) → `status: ok`, `{error: 0, warn: 0}`.
- **Reviewer:** pij-soviet-knobbler · model github-copilot/claude-opus-5 · 2026-09-02
- **SHA under review:** `f9b6d0780cd9208a844c129748cccd45beda1d8d` (detached in `/Users/jordanknight/substrate/flowspace/fs3-review-016`), base `82f60ec`, 47 files +2529/−79.

## PR head drift — docs-only, CONFIRMED

```
$ git diff --stat f9b6d07..fa4da2f
 docs/plans/016-hidden-dirs/assets/tasks/phase-1/execution.log.md | 2 ++
 docs/plans/016-hidden-dirs/assets/tasks/phase-1/tasks.dd.json    | 2 +-
 docs/plans/016-hidden-dirs/assets/tasks/phase-1/tasks.dd.md      | 2 +-
 3 files changed, 4 insertions(+), 2 deletions(-)

$ git diff --name-only f9b6d07..fa4da2f -- crates/ | wc -l
0
```

Zero paths under `crates/`. The head drift is documentation only, so this review's crate evidence covers PR #107's head unchanged. **CI is green on the reviewed sha itself**, not just the head: run `33603518927` `conclusion=success head_sha=fa4da2fe…`, and a second run `conclusion=success head_sha=f9b6d078…`.

## AC verdicts

| AC | Verdict | The load-bearing evidence I generated |
| --- | --- | --- |
| ac-0001 persistence + tri-state | **TRUE** | 4 transitions through the real CLI, each confirmed in *both* the envelope and the DB row: `--include-hidden`→t · plain `add`→t (**preserved**) · `scan`→t (preserved on the rescan path too) · `--no-include-hidden`→f, and the file map shrank back to `src/b.ts`. Preserve is structural: `register_worktree` never writes the column; `set_worktree_include_hidden` fires only on `Some`. |
| ac-0002 both call sites + deny list | **TRUE** | Live watcher, not a stub: `watcher_rescans_follow_the_current_per_root_hidden_policy` PASSED — real `WatcherSupervisor`, flag flipped by re-add, **no restart**, next rescan honours it. `watch.rs::relist` re-reads the policy *every* rescan rather than capturing it at install (risk-1 answered structurally). **My own extra case:** `.hidden/node_modules/deep.ts` — a deny-listed dir nested *inside* a hidden one is still pruned. `.git`, `node_modules`, `.venv` never mapped in either mode. |
| ac-0003 ledger + status/tree | **TRUE AS WRITTEN** | **Two roots of differing policy registered at once** — the case a cwd-only implementation fails: `status --json` returned the correct `include_hidden` for *every* root row. Ledger count is measured, not decorative (fixture 3, pij 14, each matching its pruned list exactly). Envelope strictly additive; no key renamed or removed. Caveats carried as findings f-16a1/f-16a2/f-16b1. |
| ac-0004 no regression | **TRUE** | Run by me at f9b6d07: `cargo fmt --all --check` → FMT_OK; `cargo clippy --all-targets -- -D warnings` → **exit 0, zero warnings**; targeted suites across store/daemon/parsers/cli/core all ok except `tests/health.rs`, which I **diagnosed as not-016** (below). Changed test files inspected for weakened assertions — all mechanical signature updates; the parser tests *tightened*. |
| ac-0005 real usage | **TRUE** | Re-derived from primary sources, and the author's numbers survive **to the byte**. |

## AC-0005, re-derived rather than audited

Baseline taken from git, not the receipt: `git ls-files '.pi/**/*.ts' | wc -l` = **379** (pij head `4418e3f3`).

- **Default:** `include_hidden=false`, `files=1974`, `hidden=14`; `.pi/%` rows = **0**; scoped `.pi/**` search → zero results with a path-unmatched `next_action`.
- **Opt-in:** `include_hidden=true`, `files=2463`, `binary=1`; `.pi/%` rows = 402, of which `.ts` = **378**.
- **Exact diff** (`LC_ALL=C` sorted both sides): tracked 379, stored 378, tracked-but-not-stored = **exactly** `.pi/extensions/pij/core/memorable-id.test.ts`; stored-but-not-tracked = **empty**.
- **Binary cause verified, not accepted:** that file's first 8 KiB holds **3 NUL bytes in 6322 read** — inside `SNIFF_BYTES=8192`, matching `binary=1`.
- **Search:** on a byte-identical copy at the same `.pi/extensions/pij/adapters/` path, default add could not find it; `--include-hidden` returned `…daemon-http.ts::daemonLocation`, kind function, span **[62, 72]**, score **1.0** — the exact span claimed — and `sed -n '62p'` of the real file is `export function daemonLocation(`. Flipping back to `--no-include-hidden` **reaped** it.
- *Method note:* the count half ran against the real pij repo; the search half ran on the mirror because a fresh scratch DB enqueues all 2463 files (2034 jobs) and the drain is a throughput limit on my host, not a correctness question.

## Findings (ranked)

1. **f-16a1 · MEDIUM · the prune ledger's `fix` names a command that provably does not work.** The hidden check runs *before* the deny list (`discovery.rs:868` vs `:884`), so `.venv`/`.cache`/`.next` are reported by a default add with reason `hidden` and the fix *"index hidden directories with `flowspace3 add <root> --include-hidden`"*. **Receipt — two commands, same daemon:** default add emitted `{path .venv, reason hidden, fix "…--include-hidden"}`; following that fix literally, the opt-in add emitted `{path .venv, reason standard-ignore, fix "[scan] standard_ignores = false"}` and `.venv` was **still** absent from the map. The author's `hidden_directory_prunes_name_the_effective_rule` pins the reason flip but asserts nothing about the fix, so the suite stays green while the advice is wrong. **Smallest fix:** test the deny list before the hidden policy for directories — also stops the reason flipping with the mode.
2. **f-16a2 · MEDIUM · `tree` reports the *cwd* root's policy for a file in a *different* root.** `tree_hidden_policy` (`read.rs:346-364`) resolves from `scope.worktree` while the repo comes from the resolved address. **Receipt — symmetric, both directions:** from inside a root with `include_hidden=false`, `tree el:…fixture2/src/c.ts` returned `include_hidden: true` for a root whose stored value is `false`; the mirror case returned `false` for the `true` root; and the human renderer printed `· hidden yes` for the root whose policy is off. Controls rule out coincidence: plain `tree` in each root, and `tree <absolute path>`, both answer correctly, and `--repo <other>` returns honest `null`. A wrong policy label is worse than an absent one. **Smallest fix:** resolve from the worktree owning the resolved file, or return `None` when the resolved repo isn't the cwd worktree's.
3. **f-16b1 · MINOR · `hidden=N` counts directories among per-file counts.** On pij a default add reports `hidden 14` beside `config-format 167`, while the actual omission is 379 tracked `.pi` TS files. The count *is* measured (3 on my fixture, 14 on pij, each matching its pruned list) — it just counts a different noun from its neighbours, and the same dirs are already named with a fix in the `pruned` table.
4. **f-16b2 · MINOR · the manual hidden check narrows the filter it replaced.** `.hidden(false)` + `name.starts_with('.')` after a `to_str()` bail-out loses two cases `ignore` covered: non-UTF-8 dot-named dirs (the crate compares *bytes*, `pathutil.rs:11-19`) and Windows `FILE_ATTRIBUTE_HIDDEN` (`:28-45`). Unreachable on this host — APFS rejects the name (`OSError 92`) — reachable on ext4/Windows. This is exactly the "manual prune equivalence" the author asked review to check. Verified in passing: `ignore` exempts depth 0 (`walk.rs:933`), so a *hidden root* still walks in both old and new — `add ~/repo/.harness` is unaffected.
5. **f-16b3 · MINOR · `add` is the only human surface that never shows the policy it just set.** Present in `--json`; `status` and `tree` both render it; the `add` facts table doesn't.

## Not a 016 defect — but now diagnosed (f-16c1)

`crates/daemon/tests/health.rs::the_real_binaries_agree_through_a_discovered_config` fails **deterministically** on this host — I reproduced it inside the full run *and* isolated-and-unloaded (10.24 s, `health.rs:178`), so it is **not** a load artefact as the timing suggested. **Cause found:** boot probes ddocs tooling for *every* registered worktree **before it binds** (`boot.rs:478-492`), and the shared `flowspace3_test` DB currently holds **19 registered roots**.

Controlled A/B, same binary, same config shape, both on :5434:

| registered roots | key published | outcome |
| --- | --- | --- |
| 0 (fresh DB) | **1 s** — `roots=0`, `listening bound=…:54998` | health 200 |
| 19 (`flowspace3_test`) | **25 s** — `roots=19` | test ceiling is 10 s → deterministic fail |

This **upholds o-prime's ruling** and replaces "unexplained timeout" with a mechanism. Two follow-up notes, neither a 016 defect: the pre-serve probe is unbounded in root count; and **plan 016's own AC-0005 receipt left root id 19** (`/Users/jordanknight/pi-hacking/pij`, 2463 files, `include_hidden=t`) registered in the **shared** test DB — the recipe kills its scratch daemon but never unregisters its roots, so it degrades that DB for every later seat. I did **not** clear it (shared state, not mine). This review deliberately used fresh scratch DBs to avoid adding to the pile.

## Seams (done-bar d2)

Examined and clean. `Option<bool>` survives all five hops — two CLI flags → `Option<bool>` → **key omitted entirely from the JSON body when `None`** (`client.rs:202-208`, not a null) → `#[serde(default)] Option<bool>` on `RootRequest` (so an old client body means *preserve*, not *false*) → `scan_root` → conditional store write. Collapses to `bool` only in the report and the store, which is correct. The two `DiscoverySettings` sites named in the impl-guide are the **only** two, both `|=` against the global; `add_root_with_priority` and `rescan_root` both pass `None`. Both `RegisteredWorktree` constructors and the test fixture constructor updated — no site left defaulting. Interface matches impl-guide u1 exactly, with the one documented fence deviation (`crates/store/src/worktrees.rs` does not exist; work landed in `refs.rs`/`read.rs` per asks 002-004). The one imperfect seam is `read.rs`'s tree policy resolution → f-16a2.

## Method / fence

- **Read-only on code, so no source mutations.** Every behavioural claim is instead backed by a **black-box receipt from real binaries** against scratch daemons — strictly stronger for these claims than a mutation — each with its exact reproduction command in the record. The author's mutation receipts were re-run as tests rather than re-mutated.
- Tests pinned to `FS3_TEST_DATABASE_URL=…:5434/flowspace3_test`. Scratch daemons: `FS3_CONFIG_DIR=/tmp/fs3-rev016-scratch` (:54990, DB `fs3_rev016`) and `/tmp/fs3-rev016-scratch2` (:54991, DB `fs3_rev016_mirror`), both created fresh on :5434.
- **Prod never touched:** no `:7373`, no `:5433`, no default config dir, no prod daemon key (row 169).
- No `harness checks` — the exclusive slot is not the reviewer's. Targeted `cargo test` only.
- All scratch daemons stopped, scratch DBs dropped, temp dirs removed. `flowspace3_test` left exactly as I found it.
- **Packet:** i6/i7 were stale plan-014 clones, raised at ack, ruled **VOID** by o-prime; reviewed against the 016 ACs + the three owed lists + the construction seams.
- **Dogfooded:** `flowspace3 search "render the add root report skip counts table"` returned `crates/cli/src/render/surfaces/roots.rs::render` as the top hit (0.62) — right consumer, first try. **No search miss to report.**
