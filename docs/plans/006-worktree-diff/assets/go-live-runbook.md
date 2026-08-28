# Go-live runbook — the live P3 measurement that closes ac-0003

**Runs**: after PR #50 merges and prime has rebuilt + restarted the production daemon.
**Operator**: the plan PM (pij-parliamentary-leopon).
**Why it is a runbook**: this is the only step in plan 006 that touches the
production index. Everything before it ran on private databases with fake
providers. It is written down so it can be read before it is run.

## What it will do to production

| effect | magnitude | reversible |
|---|---|---|
| creates a `poctest-` worktree of the main clone | one tree, removed by the script's own teardown (`trap … EXIT INT TERM`) | yes, automatic |
| the detector auto-registers it | one root | yes — teardown removes it, and the detector unregisters it anyway |
| scans it | ~305 files, all content identical to main | n/a — no rows created |
| edits K=8 files, uncommitted | 8 new blobs, ~149 new elements | reclaimed by GC |
| pays for divergent content only | **~8 raw vectors**, measured twice at this K | — |
| runs `flowspace3 gc` | database-wide, reap-only for unreferenced content | by design |

The 8 vectors are the entire provider cost. Identical content is free — measured
at 305 files for 0 elements, 0 summaries, 0 vectors and 0 provider round trips
(all 289 embed batches `x smart`, slowest 18 ms).

**GC is announced to prime immediately before the run**, per the standing rule
from the phase-1 P4 probe.

## Preconditions

1. PR #50 merged; `origin/main` contains the plan.
2. Production daemon rebuilt from that main and restarted — the detector must be
   the merged code, or the run measures the old binary.
3. `flowspace3 --version` from the installed binary matches the merged build.
4. The main clone is a registered root (it is; it is root #1).
5. Fleet queue quiet enough that global counters are readable — the script
   records the depth either way and the probe-attributable figures do not
   depend on it.

## The command

```bash
cd /Users/jordanknight/substrate/flowspace/fs3-worktree-diff
FS3_PROBE_CONDITION=go-live-live-stack \
  docs/plans/006-worktree-diff/assets/probes/probe.sh --k 8
```

No `FS3_PROBE_*` overrides for the daemon, database or CLI: the defaults are the
production stack and the installed binary, which is precisely what this run must
measure. `--gate-p4` is available if prime wants the pause before GC restated.

## What closes ac-0003

The receipt at `assets/probes/out/run-<UTC>/receipt.env` must show:

```
embedder=azure_openai                  ← NOT fake; the refusal must not fire
p3_marker_found_from_worktree=1        ← the control: findable from its own worktree
p3_wrong_version_leak_to_main=0        ← main is not served the worktree's code
p3_search_context_sensitive=1          ← the two checkouts get different answers
p3_unresolved_rows=0                   ← no hit without a file behind it
p3_get_context_sensitive=1             ← no regression on the surface that already worked
```

`p3_foreign_representative_exposure` will be non-zero and that is expected — it
is the hazard population the invariant was proven against, not a gate.

The p1/p4 predicates will also be measured live for the first time on the
production stack. They have already flipped on composed private runs
(`p1_auto_discovered=1`, `p1_unchanged_reconcile_jobs=0`,
`p4_root_still_registered=0`, `p4_served_after_gc=0`).

## If it fails

Do not adjust the probe to make it pass. The two honest outcomes are:

- **a predicate does not flip** — that is a defect in the merged code, found by
  the first real measurement, and it goes back to the owning seat with the
  receipt. The feature is already live at that point, so the question for prime
  is revert versus fix-forward, not whether to hide it.
- **the run refuses** (`unmeasurable-*`) — the preconditions were not met;
  fix the precondition and re-run. A refusal is not a failure.

## After

- receipt committed to the plan folder as the go-live record
- ac-0003 closed with that receipt; the expected `plan validate` contradiction
  (tk-e203 checked, ac-0003 unchecked) clears with it
- `harness plan pr-body` becomes renderable, which is its own small proof that
  the plan is genuinely closed
