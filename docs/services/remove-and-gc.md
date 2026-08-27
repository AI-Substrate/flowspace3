# remove & gc — unregistering a root, and reclaiming what nothing needs

**Owner**: pij-strange-edeard · **Requirement**: PRD req 57 (Jordan, 2026-08-27).

Two verbs that look like one and are deliberately separate.

| | `flowspace3 remove <path>` | `flowspace3 gc` |
|---|---|---|
| touches | the REF layer, and the root's own queued scans | the CONTENT layer |
| scope | one root | the whole database |
| cost | one small transaction | batched, slow cadence |
| answer | what went, and what is now reclaimable | what was freed |

## Why removal does not delete content

Workshop 002 decision D8: a worktree going away must never cascade into
re-payable LLM spend. The content layer is keyed by **content**, not by root —
forty branches holding one file share one parse and one enrichment — so "this
root left" says nothing about whether its content is still needed.

Removal therefore unregisters. GC answers the separate question of what is
genuinely unreferenced, and it answers it about the whole database rather than
about the removal that happened to precede it.

Jordan blessed the latency explicitly: *"its not end of world if some detritus
before gc runs"*. The cadence is the contract.

## What `remove` does

One transaction over: the root's `worktrees` row, its `worktree_files`
(cascade), the `repos` row **only if no other checkout of it remains**, and its
`pending`/`running` `scan_file` jobs.

**`summarize` and `embed` jobs survive**, and this is the correction that
matters. They are keyed by blob and raw hash, not by root, so they may be work
for content another registered root still holds. Killing them because one root
left is D8 violated. GC reaps the ones that turn out to be unreferenced.

### The mid-scan race, which turned out not to be one

Jordan ruled mid-scan removal first-class: *"kill the job queue for that thing
and make sure no more are processed too."* The feared part — a running job's row
being locked when the delete fires — does not happen:

`claim_job` takes its row lock inside **one autocommit statement**
(`UPDATE … WHERE id = (SELECT … FOR UPDATE SKIP LOCKED LIMIT 1)`). A running
job's row is *marked* running, not *held*. The delete never waits on a worker,
and no wait-or-mark-for-death protocol is needed.

What does need care, and is handled:

| Edge | Behaviour |
|---|---|
| Queued scans | deleted in the removal transaction |
| A scan claimed just before removal | the worker re-reads its worktree, finds it gone, and completes having done nothing — no foreign-key spray, no resurrection |
| Settling a job whose row was deleted | updates nothing, and does not error |
| The watcher | drops the root within one reconcile pass; it reads Postgres, so it needs no notification |
| Two concurrent removals | `SELECT … FOR UPDATE OF w` serialises them, so the loser reports `was_registered: false` rather than a truthful set of zero counts under a misleading headline |

### Paths are matched exactly

Roots are stored as the daemon resolved them at `add` time. On macOS `/tmp/x`
is registered as `/private/tmp/x`. `remove` matches on that stored string, so
the not-registered envelope **lists the roots that are** — turning a dead end
into something copyable.

## What `gc` does

Four levels, each re-deriving its unreferenced set from what **remains** after
the level above:

```text
0. jobs        pending summarize/embed whose raw_hash no referenced element carries
1. elements    blob_sha no worktree_files row maps
2. summaries   raw_hash no REMAINING element carries
3. embeddings  source_hash no remaining element raw_hash / summary text_hash carries
```

That ordering is the entire safety argument, because **the content layer has no
foreign keys between its levels** and `smart_content` is keyed by `raw_hash`,
not by blob. One raw hash can belong to elements of many blobs — the same
function text in two different files is exactly what content-addressed
enrichment exists to exploit. A level-two delete keyed off "the blob went away"
would destroy a summary a still-registered root depends on: D8 violated from
inside the pass written to enforce it.

Both survival cases are tested:

- a blob two repos hold survives the removal of one;
- **a summary whose raw hash is carried by elements of two different blobs
  survives the collection of one of them** — the one that actually protects
  paid-for output.

### `reclaimable` is a floor, not a forecast

Deeper levels only become collectable once the level above actually goes, so
the count under-reports what a pass will reach. Simulating the cascade
read-only would mean modelling three deletes inside one query — a second
collector, written in SQL, free to drift from the one that runs. So `remove`
says "reclaimable", and `gc` reports what it *did*.

Live, on a two-file repo: `remove` reported 4 reclaimable, the pass freed 6.

### Cadence, and why GC is a reconciler

`GcSupervisor` counts reconcile ticks rather than growing the trait a per-loop
interval — the same shape the update supervisor's clock takes. Unlike that one
the counter is **not** persisted: there is no quota to spend and nobody to be
polite to, so a redundant pass after a restart costs one cheap query per level.

Being a reconciler rather than removal's cleanup step means it reaps residue
nobody removed on purpose: a crash mid-scan, a branch switch, an old removal
from before this code existed.

## Guarding the spend

A root removed while enrichment sat queued leaves work for content nothing maps
— and summarising it pays a provider for something nobody can ever search. Two
mechanisms, because neither alone is enough:

1. **GC reaps unreferenced pending jobs.** One definition of unreferenced, one
   reaper. But GC is slow by design and the runner drains fast, so most of a
   removed repo's backlog would be paid for inside the window.
2. **The summarize handler re-checks at the point of spend.** Same predicate,
   immediately before the provider call. This also covers the case GC can never
   reach: a job already *claimed* when the removal landed.

Both ask about `raw_hash`, not blob — one raw hash stays worth paying for while
any of its blobs is still referenced.

## Where the code is

| Concern | File |
|---|---|
| Removal + GC transactions | `crates/store/src/roots.rs` |
| Envelope shaping, both verbs | `crates/daemon/src/remove.rs` |
| GC cadence | `crates/daemon/src/gc.rs` |
| Routes | `crates/daemon/src/http.rs` (`POST /remove`, `POST /gc`) |
| Spend guard | `crates/daemon/src/enrich.rs` |
| CLI | `crates/cli/src/main.rs`, `crates/cli/src/client.rs` |
| Proof | `crates/store/tests/pg_remove_and_gc.rs`, `crates/daemon/tests/remove_root.rs` |

## Open, and named rather than hidden

- **`remove --purge`** (synchronous full reclamation) is deliberately not built.
  GC's cadence covers it; add the flag only if somebody needs it to be
  synchronous.
- **A deleted directory cannot be removed by its pre-deletion path on a system
  that symlinks it.** The CLI canonicalises, which fails once the directory is
  gone, so the raw path may not match what was stored. The envelope lists the
  registered roots, which makes it recoverable rather than invisible.
- **`embed` has no spend guard**, only `summarize`. Embedding is far cheaper per
  call and its jobs are batched, so the GC reap covers it proportionately; the
  hook is the same predicate if that stops being true.
