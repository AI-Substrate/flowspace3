# w-scan-churn root-cause report

> Provenance: authored by pij-motionless-mawhrin in
> `/Users/jordanknight/substrate/flowspace/fs3-scan-churn/scratch/` (worktree
> tidied after PR #69 merged); re-homed verbatim into the main clone by
> o-prime from its read of the original. The CSV receipt files listed at the
> end did not survive the tidy — their measured numbers are quoted inline in
> this report and in `scratch/scan-throughput-review.md` §6. Fix merged as
> PR #69 (07c4003).

## Verdict

The queue is not hot-retrying one failed job and is not being repopulated by deleted roots. The dominant producer is **automatic linked-worktree discovery**:

1. `WorktreeSupervisor` runs immediately and every 30 seconds.
2. Each newly created linked Git worktree is automatically registered.
3. Registration calls `add_root_with_priority(..., JOB_PRIORITY_NEW_WORKTREE_SCAN)`.
4. `scan_root` walks the whole checkout and enqueues a distinct `scan_file` job per path whenever the new worktree lacks a path mapping, even when the same blob is already parsed at the current parser version. A new worktree lacks every path mapping, so an otherwise identical checkout still creates a near-full-tree scan batch.
5. Those jobs have priority 1. The claim index is `(priority DESC, id DESC)`, so every newly discovered coder worktree jumps ahead of the existing backlog and its newest rows are consumed first. Repeated worktree creation makes the visible count bounce and lets the oldest promoted rows age.

This is discrete hidden input from worktree creation, not duplicate insertion of live dedupe keys. Watcher activity also inserts ordinary priority-0 scans for real file changes, but the measured jumps are full promoted worktree batches.

## Production proof

### Live causal observation

- 23:39:02 UTC: `scan_file` pending 10,335; min/max id `745175/803903`; oldest pending age 27m22s.
- 23:39:53 UTC: pending 10,136; same max id `803903`; zero rows minted after the first snapshot. The queue was draining.
- At 23:40:19 UTC, worktree id 101 `/Users/jordanknight/substrate/flowspace/fs3-copilot-provider` was automatically registered.
- That one registration created **748 scan rows, all priority 1**, ids `811207..812228`.
- 23:40:31 UTC: pending rose to 10,733; max pending id rose to `812145`; 666 new pending scans existed after snapshot 1. Net count rose by 597 despite concurrent drain.
- The oldest pending row stayed id `745175` with unchanged timestamp `23:11:39`; its age grew from 27m22s to 28m51s while fresh high ids arrived. This is the LIFO starvation shape requested in the rider.
- 23:40:47 UTC: worktree 101 still accounted for 656 pending promoted scans; global oldest age reached 29m07s.

### Backlog composition

At 23:40:31 UTC:

- Priority 1 pending: **9,663**.
- Priority 0 pending: **1,066**.
- Large promoted batches correspond directly to newly discovered worktrees: 1,360 pending for `s108-w1-events`, 1,353 for `s108-w1-store`, 954 for `048-better-documentation`, 883 for `045-windows-compat`, and similar batches for the other linked checkouts.
- The earlier registration burst is visible in `worktrees.added_at`: four pij worktrees at 23:11–23:12, `flow_squared` plus eight linked fs2 checkouts at 23:15–23:17, then fs3 coder worktrees at 23:21, 23:23, 23:34, and 23:40.

### Why this is not live-key duplicate churn

- `jobs_live_dedupe_idx` is unique on `dedupe_key` for pending/running jobs.
- Repeated historical scan keys exist only as completed generations after actual later changes.
- The newest watcher-generated rows for worktree 16 either had no prior generation or had a prior completed row with a **different blob hash**. They are real changed/new files, not identical hot re-enqueues.

## Tidy/deleted-worktree interaction

- Absent worktree ids, including recent ids 80–83, retain only historical `done` scan rows.
- Pending/running scan rows whose `worktree_id` no longer exists: **0**.
- Current watcher code reconciles against the `worktrees` table, drops removed watcher handles, and forgets that root's debouncer state.
- The lifecycle supervisor requires two absence passes, then calls the existing root removal path. No evidence shows deleted fs3 worktrees repopulating the live queue.

Conclusion: tidy cleanup is not the active producer.

## Failed embed interaction

- The empty-input embed failure is one row, id `743384`, attempts 3, last updated 14:01:46 UTC, with one total generation for its dedupe key.
- It had not changed for more than nine hours during the incident and created no scan rows.
- Non-terminal failed jobs are requeued only by the daemon boot sweep (`requeue_failed`), not in a hot loop.

Conclusion: the failed embed is unrelated to scan churn.

## Fix-shape implication

The enqueue predicate, not retry backoff or deleted-root tombstoning, is the relevant layer. Automatic discovery currently pays one scan job per path for a new checkout even where `worktree_files` has already been synchronized and that blob is parsed at the current parser version. The smallest likely correction is to avoid scan work for reusable current blobs while preserving worktree-specific cases (notably ddoc/tooling-derived state), then prove a newly discovered identical checkout creates no corpus-sized scan batch and a divergent file still creates exactly the needed job. Priority/LIFO prevents fair draining but is an amplifier, not the producer.

No production state or source code was changed during this investigation.

## Receipts (original paths, not preserved — see provenance note)

prod-schema-and-samples.csv · prod-snapshot-01..04.csv · prod-priority-shape.csv ·
prod-recent-root-batches.csv · prod-fresh-scan-generations.csv ·
prod-removed-worktree-jobs.csv · prod-failed-embed.csv · worktree-reconcile.json ·
scan-root.json · watcher-supervisor.json · requeue-failed.json
