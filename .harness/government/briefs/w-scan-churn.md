# w-scan-churn — scan queue bounces instead of draining (live prod incident)

**From**: pij-instant-lynx (o-prime) · 2026-08-29 · Jordan-flagged ("there's a
problem with whatever we're doing with FlowSpace and the search stuff").

## Observed (o-prime measurements, prod daemon :7373, v0.5.0)

`scan_file` pending+running sampled every 2 min:
11141 → 10964 → 11144 → 11375 → 10913 → 10799 → 10802.
It RISES between samples — something re-enqueues scans at roughly the drain
rate. Steady state, not progress. `summarize` pending ~10.4k. Embeds keep up.

Context that may or may not be the cause (establish, don't assume):
- `/Users/jordanknight/substrate/fs2/flow_squared` (~959 files) was added ~40
  min before; a stale duplicate root at ~/github/flowspace_2 was `remove`d.
- ~10 registered fs3-* worktree roots' DIRECTORIES were deleted by
  `harness team tidy --force` while the daemon ran; `status` no longer lists
  them as roots (daemon appears to have dropped them) — but their queued jobs,
  watcher state, or discovery loop may not have been cleaned up.
- One embed job fails repeatedly: empty-string input (backlog row 68) — check
  whether failed jobs requeue hot.

## The job

1. ROOT CAUSE with a mechanism: name exactly WHAT enqueues the new scan_file
   jobs (watcher event? periodic rescan tick? discovery loop re-adding
   something? failed-job retry with no backoff? a root whose scan restarts
   because its cursor/anchor vanished?). Read the queue table directly
   (Postgres 127.0.0.1:5433, creds per crates/store defaults / docker-compose)
   — group pending jobs by root/path prefix and by created_at bucket; watch
   which paths are being RE-inserted.
2. Check the tidy interaction: do jobs referencing the deleted fs3-* worktree
   paths still exist / re-arrive? Does the watcher hold dead watches?
3. FIX at the right layer, smallest correct change: dedupe/backoff/tombstone —
   whatever the mechanism demands. A queue must converge when input stops.
4. Regression test proving the churn shape is dead (enqueue idempotence or
   backoff, whichever it is).

## Rules & fence

- READ-ONLY against the prod daemon and DB until root cause is NAMED with
  evidence; then STOP-AND-ASK o-prime (pij-instant-lynx) before any fix that
  touches prod state. Code changes go in YOUR worktree only.
- Worktree: /Users/jordanknight/substrate/flowspace/fs3-scan-churn, branch
  w-scan-churn. Absolute paths always; never `flowspace3 add` your worktree.
- Read CLAUDE.md + .agents/skills/pij-team/TENETS.md. Per-seat
  CARGO_TARGET_DIR + per-seat test DB; teardown at stand-down.
- Gate `harness checks`, commit `harness commit`, PR into main.

## Report-back

Numbered plan-of-attack first (ack-before-code); then root-cause report with
the query receipts; then the fix PR. Everything by pij send to
pij-instant-lynx with path pointers.
