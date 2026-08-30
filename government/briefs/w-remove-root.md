# Brief: w-remove-root — the remove verb (req-0057), mid-scan-safe

**Seat**: pij-strange-edeard (fourth packet — daemon verbs, queue semantics, and the
reconcile substrate are all adjacent to your domain). PR-era done-bar as always.

## What Jordan ruled (2026-08-27, binding)

1. `flowspace3 remove <path>` unregisters a root and tidies its records.
2. **Mid-scan removal is first-class**: if the root is still being indexed, removal
   KILLS its queued work and guarantees no more of its jobs are processed — "we
   should kill the job queue for that thing and make sure no more are processed too."
3. Derived-data stance (o-prime recommendation, Jordan aware): v1 is UNREGISTER —
   repo/worktrees/worktree_files/jobs go; blob-keyed artifacts (elements, summaries,
   embeddings) STAY because they are content-addressed and shared across repos.
   The envelope says so honestly (data retained under the shared-content model;
   a prune/GC pass reclaims unreferenced blobs later — that GC is a FOLLOW-UP
   reconcile producer, not this packet).

## Deliverables

1. **CLI verb** `flowspace3 remove <path>` — thin client per the house pattern
   (resolve absolute path, POST to the daemon, print the envelope). Envelope reports
   exactly what was removed (counts: worktrees, files, jobs killed) + the retention note.
2. **Daemon endpoint + store operation**: delete the root's registration rows and
   ALL its non-terminal jobs (pending, parked, running-marker rows) atomically —
   one transaction, so a crash mid-remove cannot leave a half-registered root.
3. **The mid-scan race, closed at every edge**:
   - **Queued jobs**: deleted in the removal transaction.
   - **Running jobs**: a worker mid-job for a removed root must settle HARMLESSLY —
     job completion/persistence must tolerate the root being gone (skip-and-log or
     complete-into-void; your call, but no error spray and no resurrection). Note
     the claim mechanism (FOR UPDATE SKIP LOCKED) means a running job's row may be
     locked when the delete fires — decide deliberately: wait, or mark-for-death
     and let settle reap it. Document the choice.
   - **No new work**: every enqueue path (discovery completion, watcher hints,
     re-emission, scan follow-ons) must no-op for an unregistered root. The
     level-triggered reconcile shape should make most of this free — the watcher
     supervisor already drops roots that vanish from Postgres (verify, don't assume);
     the dangerous paths are in-flight discovery results landing AFTER removal.
   - **Test the race for real**: add a root with enough files to be mid-scan,
     remove it mid-flight, assert the queue drains to zero for that root, nothing
     new appears over several reconcile passes, and the daemon logs no errors.
4. **Watcher**: assert (test) that the watcher stops watching a removed root within
   one reconcile pass.
5. **GARBAGE COLLECTION (Jordan ruled it IN, 2026-08-27: "i like garbage collection")**:
   a reconcile-loop GC pass that deletes blob-keyed derived rows (elements, summaries,
   embeddings, smart_content) whose blob SHA is referenced by NO remaining worktree.
   Level-triggered: it re-derives the unreferenced set each pass from Postgres, so it
   also reaps residue from crashes, old removals, and branch switches — not just this
   verb's removals. Constraints: batched deletes (never one giant transaction over a
   big set); a count-reporting dry-run shape doctor/status can surface; NEVER collect
   blobs still referenced by any worktree map (a shared file between two repos survives
   the removal of one — test exactly that case); GC runs on a slow cadence and must be
   cheap when there is nothing to do. Envelope for `remove` reports "reclaimable by GC"
   rather than promising instant reclamation.
6. **Docs**: service page section + the skill/agents docs mention the verb; envelope
   next_action from `status` on an unknown root should steer sensibly.

## Out of scope

`remove --purge` (synchronous full purge) — GC's cadence covers reclamation; add the
flag later only if someone actually needs synchronous. req-0057 register update flows
through o-prime's governance batch.
