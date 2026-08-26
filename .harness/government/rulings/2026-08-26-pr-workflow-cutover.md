# Ruling — PR-workflow cutover (Jordan, 2026-08-26)

> **EXECUTED 2026-08-27**: v0.2.0 shipped, fleet drained and closed, CI trigger
> moved to `pull_request`-only (commit 69d06ca), Jordan protecting main. From
> this point the operating model below is CURRENT, not queued. Coder briefs
> must state: own worktree + branch, conventional commits, PR into main as the
> done-bar; direct pushes to main are no longer possible.
Once the current work drains and the v0.2.0 release is done: main gets BRANCH PROTECTION and all
work moves to PRs from then on. At that moment (one cutover, deliberate): CI trigger moves from
push-to-main to pull_request-only, the green check becomes a merge requirement, and the push
trigger is removed (ox holds the mechanical change; noted in docs/services/ci-release.md).
Until cutover, the push-to-main gate + commit-push-as-you-go ruling stand unchanged.

## Operating model after cutover (Jordan, 2026-08-26, verbatim intent)
Same briefing discipline as today, different substrate: each coder gets its OWN git worktree +
branch for its packet ("each one can have their own work tree"), works there exactly as briefed,
and opens a PR when ready — instead of everyone committing to main. O-prime coordinates the
merging back in and the releases; worktrees are tidied up when the packet is done. Consequences:
- The shared-index/swept-stage class of incident disappears (no shared working tree).
- Fences become worktree+branch scoped; the descriptive-fence + notify-only convergence rule
  (global invariant 11) governs cross-worktree synchronization.
- Commit-push-as-you-go survives as commit-push-to-your-branch-as-you-go; conventional commits
  stay binding (release-please reads them).
- Merge order and conflict resolution are o-prime's coordination duty; releases remain
  Jordan-worded.
