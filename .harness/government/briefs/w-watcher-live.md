# Worker brief — live watcher in the daemon · pij-surprising-sailfish (re-opened, window daemon-shell)
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · Jordan's ask, verbatim intent: "the daemon should automatically watch whatever paths are present on boot, and also if I add a path, it should start watching it."

## The job
Wire real file-watching into the daemon, per YOUR doctrine (`pocs/daemon-shell/LEARNINGS.md` — lift the core/shell split verbatim) and the new **reconcile-loop doctrine** in `docs/plans/prd/daemon-worker-architecture.md` (read it first; it is a Jordan ruling).

1. **Reconcile substrate first** (`crates/daemon/src/reconcile.rs`): `Reconcile` trait (single `async fn reconcile(&mut self)` + `name()`) and the generic runner — tick cadence, immediate first pass at boot, `Notify` nudge (coalescing), error containment (failed pass logs + waits for next tick, loop never dies), shutdown, per-pass log line at debug (quiet when nothing changed). ~40 lines; no associated-type generality.
2. **WatcherSupervisor = first implementor**: reconcile pass diffs `worktrees` table (desired) against live watcher handles (actual); starts one watcher per new root, drops removed ones. Pure diff function separately tested (no clock, no DB). Boot-watch and watch-on-add both fall out of this — no special cases. Pure reconcile only (~5s cadence); NO nudge wiring yet (ruled: add-latency is invisible since `add` enqueues the initial scan directly).
3. **Events → jobs**: lift your prototype's debounce core (per-path coalesce, restart-on-event window, sweep) **plus the max-age settle** the prototype deliberately omitted. Settled path → enqueue `scan_file` via the existing store queue (content-keying makes over-reporting free); dirty DIRECTORY → re-list via the existing discovery walker and enqueue what it finds. Ignore filter (component-match `.git`/`target`/`node_modules` minimum) before the debouncer. Tolerate vanished paths (renames/deletes) — never canonicalize an event path.
4. **Registration**: composition root constructs the supervisor once, hands it to the reconcile runner (`Vec<Box<dyn Reconcile>>` shape even at n=1). Streaming visibility: reuse your 74bf8a7 log style — one info line per root watch start/stop, and watcher counters in the periodic progress line if cheap.
5. **Tests**: pure-core tests for diff + debounce/max-age; e2e in the first-light style (throwaway db): add root → touch file → job appears → element updates. Mark anything slow per the tier convention.

## Deferred (do NOT build)
Root-overlap absorb policy, inotify descriptor budgeting, non-UTF-8 boundary answer, periodic full-walk backstop, nudge handle — note them in the service page as open, they join the daemon plan.

## Rules & fence
- Fence: `crates/daemon/**`, `crates/cli/**` (only if wiring needs it), `docs/services/watcher.md` (write it, per convention). DO NOT touch `docker/**`, `release.yml`, installer files — ox has in-flight work there.
- Clean shared index: stage only at the moment you commit; conventional commits; file-scoped adds; push-first (ruling + amendments).
- `harness checks` green; arch-check crate count untouched (this is in-crate work, not a new crate).
- Report to pij-instant-lynx: claim · shas · e2e transcript · service page path. Deviations = stop-and-ask.
