# Plan 006 — worktree diff: version-correct indexing and search across worktrees

Jordan's ask, 2026-08-28: *"we need to ensure our worktree diff works — so we can
create worktrees and they auto get scanned and only changed files in there that
diff from main branch are re-scanned, searching uses those versions etc."*

## What this does

Creating a git worktree of a registered repo now makes it searchable with no
manual `add`, removing it tidies up with no manual `remove`, and search answers
from the checkout you are standing in.

**The headline, measured live on the composed build**: one root registered by
hand, **twenty auto-discovered** by the detector, ~15k jobs drained, zero
provider spend.

## Phase 1 measured before anything was built — and changed the answer

This is the first plan run in **investigate-then-build** mode. Phase 1 was four
scripted probes producing `docs/plans/006-worktree-diff/assets/probe-report.md`,
which prime signed off before a coder existed. It retired roughly half the
provisionally-planned work:

| provisional unit | outcome |
|---|---|
| u-a worktree auto-discovery | built |
| u-b diff-scoped scanning | **refuted** — identical content already costs zero elements, zero summaries, zero vectors and zero provider round trips. The saving existed; only the trigger was missing. |
| u-c version resolution | built |
| u-d removal lifecycle | **absorbed into u-a** — both ends were missing the same *detector*, not two mechanisms |
| a default result cap (asked for mid-plan) | **already existed** at `search.rs:141` |

## The two units

**u-a — worktree lifecycle detector** (`crates/daemon/src/worktrees.rs`): a
`Reconcile` implementor that enumerates live worktrees per repo with git
plumbing, diffs against the `worktrees` table, and drives the **existing**
`roots.rs` verbs at both ends. No second reference mechanism, no content
deletion — reclamation stays GC's. Removal requires two consecutive `Ok(false)`
passes; an `Err` never removes, because a false reap re-buys paid enrichment for
divergent content. Cadence default 6 ticks (30s), with the reasoning in the doc
comment: 1 tick would spawn one `git worktree list` per repo every 5s, ~52k
subprocesses a day for an event that happens weekly.

**u-c — search honours the checkout it is asked from** (`crates/daemon/src/search.rs`,
`crates/store/src/embeddings.rs`, `crates/daemon/src/http.rs`): the daemon
already resolved `scope.worktree` and `read.rs:619` already used it — `search`
did not. Now it does, filtered **before** the `LIMIT` (post-filtering under-fills
the page and still leaks when the caller's version falls outside the fetched
cap), with per-result provenance, measured truncation metadata, and an
advisory-only weak-match hint at a calibrated floor of 0.50.

## Acceptance criteria

Five closed with receipts; one is deliberately open and named below.

- **ac-0001, ac-0005** — probe report and the committed harness that produced it
- **ac-0002** — `p1_auto_discovered=1` live with no manual add; identical content
  costs zero provider round trips (289 embed batches, all `x smart`, slowest 18ms)
- **ac-0004** — `git worktree remove` alone now leads to
  `p4_root_still_registered=0` and `p4_deleted_content_still_served=0`; gc
  reclaims 149 elements + 8 vectors with orphaned-paid **0** before and after
- **ac-0006** — gate green (9 gates); **no allowlist rows needed** because no
  dependency edge was added — the diff over `arch-allowlist.toml` and every
  `Cargo.toml` across the whole plan is empty

### ac-0003 closes AFTER this merge, on purpose

Search version-correctness is a claim about **live retrieval**. First light ran
on a private database with fake providers for fleet safety, and fake vectors
carry no semantics — a query quoting the marker function's own body verbatim
scored **0.1889** against unrelated files. So the harness now emits
`unmeasurable-fake-embedder` rather than a misleading zero, and the unit's proof
is its own suites, re-verified independently on the composed tree:
`fs3-store pg_first_light` **17/17**, `fs3-daemon first_light` **14/14**.

Measuring it properly means running the composed daemon against the production
index — which *is* this feature going live for the fleet. Prime ruled that
happens at merge, with Jordan told first, and the resulting receipt closes
ac-0003 as the go-live record. `harness plan validate` reports one expected
contradiction until then (tk-e203 checked, ac-0003 not); the reason is written
into ac-0003's own note.

## Going live: what changes for the fleet at merge

The detector's first pass is a **migration**. On this machine that is ~17-20
linked worktrees registered in one burst — measured at **+14,783 job ids** —
provider-free for content identical to main, cents for genuinely divergent
files. Steady state after that is diff-only and enqueues nothing for an
unchanged worktree (`p1_unchanged_reconcile_jobs=0`).

A worktree created *during* that first migration waits behind it (measured: 89s
on this machine). Registering newest-first is the named follow-up candidate, to
be designed against the measured number rather than pre-built.

## Verification

- `harness checks` green on the composed branch, 9 gates
- u-a: daemon lib `worktrees` 10/10, `worktree_lifecycle` 1/1 on a real temp git
  repo, `config_reference` 3/3
- u-c: store `pg_first_light` 17/17, daemon `first_light` 14/14 — re-run by the
  PM on the composed tree rather than accepted from the done report
- first light: `assets/probes/out/run-20260828T035625Z/receipt.env`
- cross-model review: `assets/reviews/`

The probe harness is committed and re-runnable by any future seat; its receipt
predicates were the units' done bars, so "done" was a re-run rather than an
opinion.
