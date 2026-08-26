# Daemon worker architecture — locked direction (not yet scheduled)
**Jordan, 2026-08-26** (verbatim intent, recorded by o-prime for the upcoming daemon plan): "the daemon … will be watching a list in the database of files, and then the worker will process through them … we can parallelize LLM work as well as embedding work for ultra-fast things, which FlowSpace does really well … a worker that can do jobs off a backlog, and one of the job types will be scan a file."

## The shape

- **The queue lives in Postgres** (PRD: dirty-file work queue). The watcher (host-native, per ruling 2026-08-26) and bulk scans only ENQUEUE; they never process.
- **A worker loop drains the backlog**: generic job runner over typed jobs — `scan_file` is the first job type; enrichment (`summarize`, `embed`) are natural siblings, letting one file's scan fan out into many small enrichment jobs.
- **Parallelism is the point**: LLM calls and embedding calls batch and run concurrently (fs2 does this well — mine its batching/concurrency shapes). The concurrency-combinator roster row (`Batched`/`Throttled`/`Retry` over the two ports) is the provider-side half of this.
- Composes with everything already landed/locked: content-addressed enrichment means the queue is derivable ("elements with no smart_content row for the current model" IS backlog); the pure scanner is the `scan_file` job's core; blob-SHA keying makes duplicate enqueues harmless (idempotent jobs).

## When we get to it
This becomes the daemon plan (with the PG schema workshop deliberately deferred from plan 001). Inputs ready by then: scanner (mollusk), config (egret), migrations (cicada, landed), watcher learnings (sailfish), providers + Azure (kazimir, landed), cross-platform + docker substrate (ox).

## Queued decisions for the daemon plan (from the store landing, sylac 2026-08-26)
- **Re-queue-while-running**: `enqueue_job` on a RUNNING job pushes `not_before` but cannot un-run it — the change is picked up by the D6 reconciler, not the queue. Decide: is reconciler latency acceptable, or does the daemon need a "re-run after completion" marker?
- **Retry/backoff policy**: `fail_job` is terminal (last_error on the row; recovery = reconciler). Decide the retry schedule, attempts ceiling, and backoff at the WORKER layer — the store deliberately doesn't invent one. (`retryable` in workshop 004 envelopes feeds this.)

## Queued decisions from the live watcher (sailfish 2026-08-26 — `docs/services/watcher.md` carries the detail)
- **Periodic full-walk backstop**: the single highest-value follow-up. It is the answer to four things at once — deletions are never reaped (the watcher cannot call `sync_worktree_files` on a partial walk without reaping every path outside that subdirectory), directories still inside their debounce window are lost on restart, an edit landing between `add` and the next reconcile pass is seen by neither, and the inotify recursive-emulation race in general.
- **Root-overlap absorb policy**: overlapping roots are currently both watched, so an edit under a nested root produces two `scan_file` rows under two worktree ids. Harmless (both key content by blob) but untidy. Product answer is likely "accept the covering root, retire the covered one, re-attribute its work".
- **inotify descriptor budgeting**: one watch per directory against `fs.inotify.max_user_watches`, with a named error at the ceiling rather than a silently partial watch. `~/`-scale roots are untested.
- **Non-UTF-8 paths, answered at the boundary**: `serde` refuses rather than transcoding, so one such filename under a watched root fails with no owner. Lossy display plus a byte-exact key, or refuse by name.
- **Graceful shutdown**: neither long-lived loop (`runner::run_forever`, `reconcile::run_forever`) has a shutdown handle; both end with the process. One ctrl-c future wired to both, or leave it — a reconcile pass is idempotent and `requeue_running` sweeps half-done jobs at boot.

## Watcher doctrine (from the daemon-shell prototype, sailfish 2026-08-26 — LEARNINGS.md in pocs/daemon-shell/ carries transcripts)
- **The watcher is a HINT to rescan, never a ledger**: inotify's recursive mode is emulated per-directory — files written into a just-created directory yield NO events (git clone / npm install are exactly this). The surviving signal on all backends is the DIRECTORY event → "dirty directory = re-list that directory".
- Persist the dirty set in the jobs table (in-memory loses pending work on crash); expect 3-5x event amplification per touch; renames report both names; deletes leave dirty entries for paths that no longer exist — consumers tolerate all three.
- Debounce needs a MAXIMUM AGE: a file touched faster than the window never settles otherwise.
- Root overlap: prototype refuses; product answer is likely "absorb the covered root". Budget inotify watch descriptors before ~/-scale roots. Non-UTF-8 paths: refuse at the boundary (serde won't transcode).
- Keep the prototype's core/shell split verbatim: pure debounce/dirty-set logic, thin notify+axum shells.

## Reconcile-loop doctrine (RULED by Jordan, 2026-08-26 — the house synchronization pattern)
- **State lives in Postgres; every consumer runs a reconcile loop against it; events, where they exist at all, only wake the loop early and never carry the truth.** Level-triggered correctness, edge-triggered latency. This is the same shape three subsystems independently arrived at (fs-watcher hints, derivable enrichment backlog, root sync) — now it is the default for anything that needs to "find out about" state.
- **Mechanics are written once**: a `Reconcile` trait with a single `async fn reconcile(&mut self)` (one idempotent pass: read desired from PG, diff with actual, apply) + one generic runner owning cadence, immediate first pass at boot, `tokio::sync::Notify` nudge handle (coalescing), error containment (a failed pass logs and waits for the next tick — the loop never dies), shutdown, and the per-pass log line. Home: `crates/daemon/src/reconcile.rs`; promote to fs3-core only when a non-daemon consumer exists.
- **Composition root registers by behaviour**: each driver takes its own roster as a plain argument (`Vec<Box<dyn Reconcile>>` to the reconcile runner, job-kind map to the job runner, routes to the router). No unified registry, no auto-discovery — construction happens exactly once at boot in one readable function; sharing is explicit `Arc` handles split off before ownership moves.
- **Scope guard**: reconcile loops synchronize STATE; they never dispatch WORK (the job queue's `SKIP LOCKED` claim loop owns that, at its own cadence). No speculative generality — one trait method, one runner; extract more structure only at the third implementor.
- First implementor: the **WatcherSupervisor** (roots from `worktrees` table → live per-root watchers). Pure reconcile first (a few seconds of add-latency is invisible — `add` enqueues the initial scan directly); the nudge lands only if lag ever becomes visible.
