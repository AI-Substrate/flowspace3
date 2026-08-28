# worktree lifecycle — discovering linked checkouts

**Code**: `crates/daemon/src/worktrees.rs`  
**Tests**: module tests plus `crates/daemon/tests/worktree_lifecycle.rs`

## Contract

`WorktreeSupervisor` implements the daemon's existing `Reconcile` trait. A
scheduled pass reads registered roots, groups them by repository identity, asks
Git once per identity for linked worktrees, and diffs those paths against the
store. Appeared paths go through the existing root-registration path with its
initial `scan_file` jobs promoted; vanished paths go through `remove::remove`.
The supervisor never deletes content and never invokes GC. Reference removal
and later reclamation therefore remain the existing `fs3_store::roots`/GC
mechanism.

The shared queue has a closed two-level priority scale in
`fs3_store::jobs`: priority 0 is ordinary explicit add/rescan, watcher, and
enrichment work; priority 1 is reserved for initial scans of a root newly
discovered by this supervisor. The claim query already orders priority
descending, so the new checkout's files jump an older scan backlog. Live-job
dedupe keeps the higher priority on conflict, preventing an ordinary re-fire
from demoting promoted work. Any future lane must declare another named value
and its preemption policy beside these constants rather than passing an
anonymous integer.

Git is the authority for membership. The process runs:

```text
git -C <one existing registered root> worktree list --porcelain -z
```

A filesystem crawl cannot prove that a neighboring directory belongs to the
same repository, while the store cannot name a checkout it has never seen.
NUL-delimited porcelain handles spaces without parsing display quoting. One
anchor is selected per identity, so a repository with several registered roots
still starts one subprocess per scheduled pass.

Git porcelain does not expose checkout creation time. Appeared roots therefore
sort newest-first by the `.git` marker's filesystem birth time. Where birth
time is unavailable, a linked worktree uses that marker file's mtime; it is
created with the checkout and normally stays untouched by source edits. The
main checkout's mutable `.git` directory is treated as oldest on that fallback.
This is the closest portable signal, not an exact Git ledger: `git worktree
repair` may rewrite a linked marker. Equal timestamps break by canonical path
ascending, so coarse timestamp filesystems remain deterministic.

The supervisor runs immediately at boot, then every
`indexing.worktree_reconcile_ticks` shared runner ticks. The default is six:
30 seconds at the runner's five-second cadence. Worktree creation is rare, so a
Git subprocess per repository every five seconds would be permanent churn for
little useful latency; 30 seconds matches the probe's discovery window. Zero
disables automatic discovery.

## Steady state and removal safety

The path diff happens before `add_root`. An unchanged registered worktree calls
neither add nor remove, performs no file walk, and enqueues zero jobs. First
registration may remain O(files); later unchanged passes are O(repositories +
worktrees).

On the first pass after enabling this service, every previously unregistered
linked checkout is an addition. Registration remains sequential through the
existing verb, but the appeared set is newest-first so the checkout a user just
created becomes searchable before older stragglers. Before this ordering, a
measured bootstrap added 17 linked checkouts beside the registered main root,
advanced the job-id sequence by 14,783, and reached the newly created 18th path
after 89 seconds. Later unchanged passes retain the zero-enqueue bound.

An I/O or permission error while checking a registered path fails the pass and
is retried. `Ok(false)` must occur on two consecutive scheduled passes before
unregistration. The in-memory streak resets when the path returns and after a
daemon restart. This trades up to one extra cadence of stale search results for
avoiding re-registration and re-paid divergent enrichment after a transient
absence.

**Assumption**: portable filesystem APIs cannot distinguish a deleted path from
an unmounted volume whose mount path now returns `ENOENT`. The two-pass grace
reduces that risk but does not identify mounts. Persistent tombstones or
platform mount inspection are outside this service's contract.

## Snap-in recipe

The unit is wired by these exact lines:

```rust
// crates/daemon/src/lib.rs
pub mod worktrees;

// crates/daemon/src/boot.rs, after constructing the reconciler roster
reconcilers.push(Box::new(
    crate::worktrees::WorktreeSupervisor::new(state.clone()),
));
```

Configuration shape:

```toml
[indexing]
worktree_reconcile_ticks = 6 # 0 disables; shared runner tick is five seconds
```

An isolated live probe requires all four axes together: a seat-unique database,
an empty config directory (therefore built-in fake providers), a unique port,
and a worktree-local log directory. Before registering a root, read the daemon's
boot line and require `embedder=fake summarizer=fake`; a database override alone
does not suppress ambient provider configuration.

```sh
FS3_CONFIG_DIR=<worktree>/.probe-config \
FS3_DATABASE__URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_<seat> \
FS3_DAEMON__URL=http://127.0.0.1:7383 \
FS3_DAEMON__LOG_DIR=<worktree>/.probe-logs flowspace3 daemon
```

Create and drop `flowspace3_<seat>` around the run; the shared
`flowspace3_test` database is not isolated in practice.

## Proof

- `cargo test -p fs3-daemon --lib worktrees` covers NUL porcelain parsing,
  per-repository enumeration, diff-only no-op behavior, cadence, two-pass
  absence, recovery, and unreachable-path refusal.
- `cargo test -p fs3-daemon --test worktree_lifecycle` uses a real temporary Git
  repository and throwaway Postgres database to prove create, unchanged, and
  remove transitions through the production supervisor.
- `docs/plans/006-worktree-diff/assets/probes/probe.sh` records automatic
  discovery/removal plus a worktree-scoped job delta for repeated unchanged
  passes. `FS3_PROBE_DAEMON_URL`, `FS3_PROBE_DB_NAME`, and
  `FS3_DAEMON__LOG_DIR` select that isolated daemon's surfaces; unset values
  preserve the composition probe's existing live-stack defaults.
