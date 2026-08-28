# PM3 ack — inventory verdict + numbered plan (pij-pale-silkworm, 2026-08-28)

Seat: pij-pale-silkworm · spawnId s1787880073435-84015 · github-copilot/claude-opus-5,
effort high · worktree ../fs3-convo-ingest, branch 005-convo-ingest @ 460883d.

## Inventory verdict — measured, not inherited

Evidence: `git worktree list`, per-worktree `git log 460883d..HEAD`,
`git status --short --ignored=matching`, `find -newermt 2026-08-27`.

| worktree | commits past 460883d | dirty tree | recent files | verdict |
| --- | --- | --- | --- | --- |
| ../fs3-convo-u1a | none | clean | none | **NOTHING SURVIVED** |
| ../fs3-convo-u1b | none | clean | none | **NOTHING SURVIVED** |
| ../fs3-convo-u1d | none | clean | none | **NOTHING SURVIVED** |
| ../fs3-convo-u2 | 5f36dc7, 016fbff | clean | — | **COMPLETE**, 1821 insertions across 9 files |

**Correction to successor-context-pm3.md.** It records u1b and u1d as leaving
partial work with "sources intact". They did not. All three dead coder
worktrees are byte-for-byte the dispatch commit: zero commits, zero tracked or
untracked modifications, zero files touched since the fan-out. There is no
half-written state to treat with suspicion and no delta worth writing into a
successor packet — the three successors start from their packets, clean.

Only `target/` differs (u1a 184M, u1b 2.8G present; u1d's gone) — build
artefacts, not work.

**u2 is real.** `git diff --stat 460883d..016fbff` on branch 005-convo-u2:

```
crates/core/src/conversation_normalize.rs        595 +
crates/core/src/lib.rs                             4 +
crates/store/migrations/0014_ingest_cursors.sql  105 +
crates/store/src/ingest_cursors.rs               301 +
crates/store/src/lib.rs                            1 +
crates/store/tests/pg_ingest_cursors.rs          555 +
docs/services/convo-cursors.md                   250 +
packet-coder-u2.dd.{json,md}                      30 +/-
```

Its own gate claim is not re-verified yet; that happens at composition, on the
composed tree, which is the only tree whose verdict binds.

**Observation buffers.** u1a/u1b/u1d: no `.harness/temp/agent/` at all —
nothing was lost. u2's WAS still in its worktree, undrained, contradicting the
handover note that said it had been rescued; I have relayed it into the shared
buffer as DL-004 (the disk-exhaustion blocker, verbatim from slug's record,
attributed to it). Nothing has been cleared — the drain is o-prime's.

## Numbered plan

1. **Rule u2 and take ownership of it.** Message pij-appalling-slug that PM3
   owns 005; rule its two declared deviations formally (pg test file outside the
   literal fence; no crate-root re-export). Both read as sound/optional from the
   packet, but the ruling gets recorded, not assumed.
2. **Respawn u1a, u1b, u1d.** Same committed packets (`packet-coder-u1{a,b,d}.dd.md`),
   delivered as PATHS. Delta note per seat is one line: *your predecessor left
   nothing; the worktree is the dispatch commit.* Spawn carries: `CARGO_INCREMENTAL=0`,
   the `rustc-LLVM ERROR: IO failure on output stream` = disk-not-code signature,
   `export PIJ_SESSION_ID=<own id>` before any `pij send`, no `docker compose up`
   (shared PG on :5433), and their scratch db from the roster. Canary-verify each
   (identity confirmed by asking the seat — pij#19 is live), require
   ack-with-numbered-plan, rule by number before any code.
3. **Update the durable roster** (`assets/wave-1-roster.md`) with the new seat
   ids and the inventory facts above, and commit it — the roster is the thing
   that made this recovery cheap and it must stay true at every seat edge.
4. **Gate-watch wave 1** with status cards at each edge; chase stale cards via
   unscoped `pij anomalies`. Watch free disk on every seat report.
5. **Phase 3 composition (mine).** Merge in dependency order — u2 first (no
   deps), then u1a/u1b/u1d — paste each unit's snap-in recipe at the composition
   root (including u2's recipe step 2: delete the daemon's duplicate
   `OUTPUT_HEAD_BYTES` and delegate to core), wire the orchestrator (tk-c302),
   `harness checks` green on the composed tree, then the integration proof:
   fixture e2e plus the tk-c305 first-light live transcript. Unit-internal
   rework at this seam is a phase-1 contract defect — stop, record, get it ruled.
6. **Review + close-out.** Reviewer from `packet-reviewer.dd.json` on the
   default (github-copilot/gpt-5.6-sol); fix or refute every finding with
   evidence; ddoc task/AC checks with receipts as I go; `harness plan validate`
   clean; PR into main held UNMERGED for prime; coders stood down with their
   buffers rescued FIRST; completion + process-feedback report.

## One question, with evidence

Disk. The event that killed four seats is not retired, only postponed: 21G free
now, and the four wave-1 worktrees will rebuild ~3-7G of `target/` each as soon
as the successors compile — u2's alone is already 6.7G. The reclaimable mass
sits outside my fence: flowspace3 36G, fs3-watcher-ignore 7.7G, fs3-team-ext
5.9G.

**Ask:** do I have your authority to `cargo clean` the idle non-005 worktrees
(and, if so, which), or would you rather the 005 fleet share one
`CARGO_TARGET_DIR` and accept that concurrent coder builds serialise on the
target lock?
