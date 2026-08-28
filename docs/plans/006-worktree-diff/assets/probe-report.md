# Probe report — plan 006 worktree-diff, phase 1 (INVESTIGATE)

**Author**: pij-parliamentary-leopon (PM, plan 006) · **Date**: 2026-08-28
**Definitive run**: `assets/probes/out/run-20260828T024227Z`
**Harness**: `assets/probes/probe.sh` (committed; one command runs all four probes)
**Status**: delivered to prime (pij-instant-lynx) for the phase-2 unit ruling — the HARD gate. No coder exists yet.

Every number below is produced by the committed script. Where I learned something
by hand first, I put it into the script and re-ran, because a claim the harness
cannot reproduce is not evidence.

---

## Method, and what it costs to trust it

- Ruled by prime 2026-08-28: probes run against the **live shared stack**
  (daemon on `:7373`, Postgres `flowspace3-db:5433`) — fidelity is the
  experiment; an isolated stack would measure a daemon nobody runs.
- The script is **read-only on the database**. Its only mutations are two
  documented CLI verbs (`flowspace3 remove`, `flowspace3 gc`) plus the git
  worktree it creates and always removes. Teardown fires on `EXIT INT TERM`.
- Probe worktrees use `poctest-` slugs. Each run creates one, registers it,
  edits K=8 files, and removes it.
- Evidence channels: DB snapshots (`snap-*.env`), job deltas (`*-jobs.txt`),
  CLI envelopes (`*.json`), and daemon-log slices captured from the daemon's
  tmux pane (`daemon-*.log`).
- **Measurement hazard, named**: the queue is global. During run
  `20260828T022257Z` another seat registered a large root and 5,254 summaries
  and 5,256 vectors landed inside my measurement window. The script therefore
  reports both a global figure and a **probe-attributable** figure (enrichment
  whose content is reachable from the probe worktree, and the subset no other
  checkout references). The definitive run below was taken with the fleet queue
  at 0, and its global and attributable figures agree.
- Four runs exist; three were discarded and why is part of the record:
  `015845Z` proved P1/P2 but its P3/P4 queries targeted a **doc comment**,
  which never enters an element — so those searches proved nothing;
  `020622Z` was killed after 15 minutes stuck behind another seat's queue;
  `023756Z` had a **broken predicate** (it diffed whole envelopes, and score
  float jitter made it report a version-resolution that does not exist).

### Observed tool versions (receipt)

| | |
|---|---|
| flowspace3 | 0.4.0 |
| cargo | 1.95.0 (f2d3ce0bd 2026-03-21) (Homebrew) |
| git | 2.51.0 |
| docker | 29.4.0 |
| host / disk avail | JordansacStudio.localdomain / 264Gi |
| main root | `/Users/jordanknight/substrate/flowspace/flowspace3` (456 files) |

---

## P1 — Is a new worktree discovered? **No.**

`git worktree add` of a registered repo, then 30 s of quiet (six watcher
reconcile passes at the 5 s cadence):

| measure | value | evidence |
|---|---|---|
| probe worktree in `status` roots | **0** | `receipt.env: p1_auto_discovered=0` |
| new `worktrees` rows | 0 | `p1-deltas.txt` |
| new jobs of any kind | 0 | `p1-jobs.txt` |

Corroborating structural fact from preflight: five live `fs3-*` worktrees exist
on this machine today and **none** is a registered root.

The daemon is not blind to the situation — it is explicit about it. A search run
from inside the unregistered worktree answers from the main checkout and says so:

> `this checkout (…/poctest-…) is not registered; git:github.com/AI-Substrate/flowspace3 is indexed from …/flowspace3, so the content answering you is that checkout's — flowspace3 add …`
> — `p1-search-from-unregistered.json`, `meta.scope.warnings[0]`

That is bobolink's finding (2026-08-28), reproduced and given its precise shape:
a worktree's content is invisible **until the whole tree is added as a root**,
and the product already tells you so in the steer.

## P2 — What does indexing a worktree cost? **Nothing, for identical content.**

### P2a — register the untouched worktree (305 indexable files)

| measure | delta | evidence |
|---|---|---|
| `worktree_files` rows | **+305** | `p2a-deltas.txt` |
| jobs created | **+4,767** rows: 305 `scan_file`, 2,430 `embed`, 2,032 `summarize` (the id sequence advanced 4,780 — identity sequences gap) | `p2a-jobs.txt` |
| `elements` | **0** | `p2a-deltas.txt` |
| `smart_content` (summaries) | **0** | `p2a-deltas.txt` |
| `embeddings_1024` | **0** | `p2a-deltas.txt` |
| probe-attributable spend | **0 summaries, 0 vectors** | `p2a-paid.txt` |
| provider round trips | **0** — all 289 embed batches were `x smart jobs=1` (zero `x raw`), the slowest 18 ms; the slowest `summarize` settle was 125 ms, orders below an LLM call. The only measured network latency in the run was P2b's single 455 ms raw batch | `daemon-p2a.log` |

**This closes ac-0002's cost half: registering a worktree whose content matches
main is free of provider spend, measured, not inferred.**

The mechanism is in the code and matches the measurement:
`crates/daemon/src/enrich.rs:552-567` filters `existing_embedding_hashes` and
**returns before the provider call** when nothing is missing ("an empty batch
that still made the round trip would have fixed the accounting and not the
bill"); `:583` then applies the reference-based spend guard.

**What it does still cost**: 4,767 job rows and 305 map rows per registration —
work that is re-emitted on purpose (dirty-is-a-missing-row) and executed for
free. Cheap per row, but it is O(files) per worktree and it is the reason
another seat's registration blocked my measurement for 15 minutes. See the
open question for prime below.

### P2b — edit K=8 files (uncommitted)

| measure | delta | evidence |
|---|---|---|
| `scan_file` jobs | 8 | `p2b-jobs.txt` |
| `elements` | +149 (all 8 files re-parsed under new blobs) | `p2b-deltas.txt` |
| summaries | **0** (the marker functions carry `enrich=false`) | `p2b-deltas.txt` |
| raw vectors | **+8** — exactly one per genuinely new function | `p2b-deltas.txt` |
| probe-attributable | 8 vectors, **8 of them divergent-only** | `p2b-paid.txt` |
| provider round trips | **1** (`batch kind="embed" subject=141 x raw jobs=13 ms=455`) | `daemon-p2b.log` |

Blob dedupe and enrichment re-emit are separable and both were measured: 141
raw subjects were offered to the embed handler; 8 survived the dedupe filter and
one provider call bought them. The store's own view agrees — 8 paths now carry
two versions each (`p2b-divergent-paths.txt`), 305 files / 304 blobs on the
probe root against 456 / 454 on main (`p2b-reference-map.txt`).

## P3 — Is search version-correct? **No. `get` is; `search` is not.**

This is the plan's real gap, and it is one seam deep.

| question | answer | evidence |
|---|---|---|
| Same query from main vs from the worktree | **identical answer** (same hits, order, paths; only float jitter differs) | `receipt.env: p3_search_context_sensitive=0` |
| Worktree-only code served to a caller standing in main | **yes — 1 hit at 0.7730** for a function that has never existed in main | `receipt.env: p3_wrong_version_leak_to_main=1`, `p3-marker-from-main.json` |
| `flowspace3 get <address>` on a divergent file | **version-correct**: span `[1,262]` from main, `[1,270]` from the worktree | `receipt.env: p3_get_context_sensitive=1`, `p3-get-from-*.json` |
| Per-result provenance | **absent**: results carry `address, kind, match_field, name, path, repo, score, smart, snippet, span, subkind, tags` — nothing names the checkout | `p3-result-fields.txt` |
| Explicit scoping available | `--repo <identity>` and `--path <glob>` only; both checkouts share ONE identity, so `--repo` cannot separate them | `p3-search-flags.txt`, `p3-search-repo-scoped.json` |
| Address collision | one address, two versions, e.g. `crates/cli/src/client.rs::DaemonClient::add` → 2 element rows / 2 blobs | `p3-colliding-addresses.txt` |

**The daemon already knows the answer it is not using.** `scope::resolve`
(`crates/daemon/src/scope.rs:153-162`) returns
`worktree: Some(<the caller's registered root>)` and the search envelope carries
it in `meta.scope`:

```json
{"cwd":"…/poctest-…","repo":"git:github.com/AI-Substrate/flowspace3",
 "source":"cwd","worktree":"…/poctest-…"}
```

- `crates/daemon/src/read.rs:619` — `get` **uses** it: with >1 blob for a path it
  prefers the caller's root, and errors with a candidate list when it cannot.
- `crates/daemon/src/search.rs:194,210` — search uses `scope.repo` **only**;
  `scope.worktree` is reported and never applied.

So the gap is not missing machinery. It is one surface not honouring a decision
the neighbouring surface already makes.

### Refutation of a scope addition, with evidence

Jordan's cap ask ("search should automatically limit to five or ten items") is
**already satisfied in substance**: `crates/daemon/src/search.rs:141` is
`request.limit.unwrap_or(10)`, with a `MAX_LIMIT` ceiling above it, and the
measured default is 10 (`receipt.env: default_search_result_count=10`). What is
actually missing is **truncation visibility** — nothing in the envelope says the
answer was capped. That is what u-c should build; building a cap would be
building what exists.

## P4 — Does removal dereference and reclaim cleanly? **Yes, once told. It is never told.**

| step | result | evidence |
|---|---|---|
| `git worktree remove` (fs3 not told) | root **still registered**; 305 `worktree_files` rows intact; zero rows moved | `receipt.env: p4_root_still_registered=1`, `p4-deltas-git-remove.txt` |
| search after the directory vanished | **still serves the deleted worktree's code** (1 hit) | `receipt.env: p4_deleted_content_still_served=1` |
| `flowspace3 remove` | 305 files unmapped, worktree row gone, `repo_removed:false`, 149 rows reported reclaimable, 0 jobs killed | `p4-fs3-remove.json` |
| orphaned PAID enrichment before gc | 0 summaries, 8 vectors | `p4-orphaned-before-gc.txt` |
| `flowspace3 gc` | reclaimed **149 elements + 8 vectors** (157 rows), 0 summaries | `p4-gc.json` |
| after gc | orphaned-paid **0**; marker elements left **0**; content no longer served | `p4-orphaned-after-gc.txt`, `receipt.env: p4_served_after_gc=0` |

**ac-0004's mechanism half is already true**: divergent-only content becomes
unreferenced and is reaped with zero orphaned paid enrichment, and shared
content is untouched (`repo_removed:false`). The `reclaimable: 149` vs freed
157 gap is the documented floor-not-forecast behaviour
(`docs/services/remove-and-gc.md`), not a defect.

What is missing is the same thing P1 is missing, at the other end: **nobody
tells fs3**. Until someone runs `flowspace3 remove` by hand, a deleted worktree
stays registered and keeps answering queries with code that is not on disk.

---

## The gap list

| # | gap | measured | severity |
|---|---|---|---|
| G1 | Worktree creation is not detected; a new worktree is unsearchable until a manual `add` | P1: `p1_auto_discovered=0` | the ask |
| G2 | Worktree removal is not detected; the root stays registered and keeps serving deleted code | P4: `p4_root_still_registered=1`, `p4_deleted_content_still_served=1` | the ask |
| G3 | `search` ignores `scope.worktree`, which the daemon has already resolved, so callers get another checkout's version | P3: identical answers both sides; `p3_wrong_version_leak_to_main=1` | the ask |
| G4 | No per-result provenance: a hit cannot say which checkout served it, and one address maps two versions | P3: `p3-result-fields.txt`, `p3-colliding-addresses.txt` | the ask |
| G5 | Truncation is invisible: default cap of 10 exists, envelope never says the answer was capped | `search.rs:141`, `default_search_result_count=10` | Jordan's cap ask, restated |
| G6 | Registration enqueues O(files) no-op enrichment jobs (4,462 of 4,767 for 305 files); free in spend, not free in queue | P2a `p2a-jobs.txt` vs zero row deltas | open question, below |

**Not a gap** (measured, so it does not get built): diff-scoped scanning.
Content-addressing already makes identical content cost zero elements, zero
summaries, zero vectors and zero provider round trips. The saving exists; only
the trigger is missing.

---

## RULED phase-2 unit set — for prime's sign-off

Two units, both wave 1, **disjoint files**, no shared edit except one
composition-root line.

### u-a — worktree lifecycle detector (absorbs the provisional u-a AND u-d)

The symmetry is the argument: **both ends are missing the detector, not the
mechanism.** `add` already scans a worktree for free; `remove` + `gc` already
dereference and reclaim it cleanly; neither is ever invoked because nothing
watches for worktrees appearing and disappearing. One reconcile pass answers
both, in the shape the daemon already uses for roots
(`docs/services/watcher.md`: "reconcile, don't react") and for GC
(`GcSupervisor` counting ticks).

Splitting create and remove into two units would mean two seats editing one
reconcile pass and one config surface, for one shared question ("which live
worktrees of registered repos exist?"). That is the collision surface tenet 3
says to design away.

- **Responsibility**: diff live git worktrees of registered repos against the
  `worktrees` table; register + scan what appeared, unregister what vanished.
- **Paths owned**: new `crates/daemon/src/worktrees.rs`; one roster line in
  `crates/daemon/src/boot.rs`; config knob in `crates/core/src/config.rs`;
  tests in `crates/daemon/tests/`.
- **Interface, frozen**: implements the existing `Reconcile` trait; calls the
  EXISTING `roots.rs` register/remove verbs — **no second reference mechanism**
  (plan risk r2, `docs/services/remove-and-gc.md`). Reclamation stays GC's, on
  GC's cadence; the detector never deletes content.
- **Concurrency/cadence declared, not defaulted** (tenet 6): its own tick count,
  named in config beside `indexing.debounce_seconds`.
- **Done predicate**: `probe.sh` re-run flips `p1_auto_discovered` 0→1 and
  `p4_root_still_registered` 1→0 **without any manual CLI call**, with
  `p4_served_after_gc` still 0.

### u-c — search honours the checkout it is asked from

- **Responsibility**: apply `scope.worktree` in search the way `read.rs:619`
  already does; label each result with the checkout that served it; make
  truncation visible.
- **Paths owned**: `crates/daemon/src/search.rs` (+ a store query if ranking
  needs the worktree join), result struct in the same file, tests in
  `crates/daemon/tests/`.
- **Interface, frozen**: `Scope` is unchanged — it already carries `worktree`.
  Additive envelope fields only (tenet 6): a per-result `worktree`, and a
  `meta` truncation statement. `--repo`/`--path` semantics unchanged; any new
  scoping flag is additive and defaults to today's behaviour.
- **Three deliverables**: (1) version resolution — prefer the asking checkout's
  version of a divergent path; (2) provenance — every hit names its checkout;
  (3) truncation visibility — the cap (already 10, `search.rs:141`) named in the
  envelope when it bites. Jordan's cap ask lands here as (3), not as a new cap.
- **Done predicate**: `probe.sh` re-run flips `p3_search_context_sensitive` 0→1
  and `p3_wrong_version_leak_to_main` 1→0, with `p3_get_context_sensitive`
  still 1 (no regression on the surface that already works).

### Ruled OUT

- **u-b diff-scoped scanning** — refuted by P2a. Fold the trigger into u-a.

### Test-strategy constraint binding on both units

Assertions compare **result identity** (address/path/name/kind/span/snippet) or
rank order — **never raw scores**. Scores carry float jitter across identical
calls (0.7730240968 vs 0.7730564295, same query seconds apart), and it already
produced one phantom finding in this phase (`023756Z`). Recorded as DL-002.

### Collision surface

`worktrees.rs` + `boot.rs` (u-a) against `search.rs` (u-c). The only shared file
is `boot.rs`, and only u-a touches it. Composition is one merge in either order.

---

## Open question for prime (needs a ruling, not a unit)

**G6**: registering one 305-file worktree emits 4,767 jobs, of which 4,462 are
enrichment jobs that store nothing. That is deliberate (re-emission is the
crash-safety doctrine) and free in provider spend. But it is O(files) per
worktree, and with the lifecycle detector of u-a it becomes **automatic and
per-worktree** — five live worktrees on this machine would mean ~24k no-op jobs
per full sweep, and one seat's registration already blocked another seat's
measurement for 15 minutes (DL-001). Is that acceptable, a PRD row, or in
scope for u-a? My recommendation: acceptable for this plan, PRD row for the
queue's per-root visibility, revisit if the sweep shows up in a profile.

---

## Limits of this evidence

- One machine, one repo, macOS, 305-file worktree, K=8 edits. No monorepo, no
  Linux, no bare-clone worktree, no worktree of a repo whose main root is not
  registered.
- Provider-call counts are inferred from batch latency plus stored rows: the
  daemon logs batches, not provider calls. Zero-spend claims rest on
  latency < 18 ms plus zero stored rows plus the code path at
  `enrich.rs:552-567`. A counter at the provider boundary would make this
  direct rather than triangulated.
- The shared stack means another seat can move global counters inside a window;
  the attributable queries are the defence, and the definitive run was taken at
  queue depth 0.

## Re-running this

```bash
docs/plans/006-worktree-diff/assets/probes/probe.sh          # all four probes
docs/plans/006-worktree-diff/assets/probes/probe.sh --gate-p4  # pause before gc
```

Requires: daemon up (`flowspace3 ping`), `flowspace3-db` reachable, the main
clone registered. It creates and always removes its own `poctest-` worktree.
Evidence lands in `assets/probes/out/run-<UTC>/`; `receipt.env` is the summary
every claim above is keyed to.
