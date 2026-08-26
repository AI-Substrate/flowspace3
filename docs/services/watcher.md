# watcher — how the daemon finds out that a file changed

**Code**: `crates/daemon/src/watch.rs` (shell) + `crates/daemon/src/debounce.rs` (pure core) + `crates/daemon/src/reconcile.rs` (the loop)
**Tests**: `crates/daemon/tests/watcher.rs` (end to end, real OS watcher, throwaway database) + the `#[cfg(test)]` modules in all three source files
**Rulings**: daemon-native-on-host (2026-08-26) · reconcile-loop doctrine (`docs/plans/prd/daemon-worker-architecture.md`) · watcher doctrine, from the `pocs/daemon-shell` prototype

## What it is

Jordan's ask, verbatim: *"the daemon should automatically watch whatever paths
are present on boot, and also if I add a path, it should start watching it."*

Both are the same code. A reconcile pass compares the `worktrees` table
(desired) against the OS watchers that exist (actual) and applies the
difference. "Already registered at boot" is the pass that runs immediately;
"added a second ago" is the next pass. There is no boot path and no add hook.

One pass, in order:

1. **diff roots** — `list_worktrees` vs live handles; start what is new, drop what is gone
2. **absorb** — drain the channel `notify` has been filling from its own thread
3. **sweep** — ask the debouncer which directories have gone quiet
4. **re-list** — walk each one, blob-diff it, enqueue `scan_file` for what actually changed

## Why the unit of work is a DIRECTORY

Because of the finding the prototype existed to produce
(`pocs/daemon-shell/LEARNINGS.md` §1): **`notify`'s recursive mode is emulated
on inotify**. It adds one watch per directory by walking the tree, so files
written into a directory created a moment ago produce *no events at all* — the
prototype's Linux run saw 98 of 100. `git clone`, `npm install` and
`git checkout` are exactly that shape, so the common case is the racy case.

The signal that survives on every backend is the **directory's own event**. So
every event is keyed by its enclosing directory, and a dirty directory means
*re-list that directory* — never *re-read that file*. `discover` walks
recursively, so re-listing an ancestor covers everything under it.

Taking the parent unconditionally also sidesteps a question that has no answer:
deciding whether an event path was a file or a directory means asking the
filesystem, and a delete event names a path that is already gone.

It is also much cheaper. A 100-file burst measured **341 raw events** on macOS;
keyed by directory that is one pending entry and one walk.

## Two debounces, at two levels, neither redundant

| Level | Mechanism | What it collapses | Durable? |
|---|---|---|---|
| Job | `enqueue_job`'s live-dedupe index + `not_before = GREATEST(existing, new)` | repeated `scan_file` rows for one path | yes — Postgres |
| Walk | `debounce::Debouncer` (in memory) | events into quiet directories | no |

The queue already debounces the *job*, and this module gets that for free by
enqueuing through the same call `roots.rs` uses. What the queue cannot decide is
**when to pay for a directory walk**, which is what an event actually costs
here — hence the in-memory debouncer. The enqueue that follows passes
`Duration::ZERO`, exactly as an explicit `add` does, because by then the wait
has already happened.

The window is `indexing.debounce_seconds` (default 10). That setting shipped
with the config and **had no reader until this landed**.

## Key decisions

| Decision | Why |
|---|---|
| **Reconcile, don't react** | State lives in Postgres and every consumer diffs against it (doctrine). A dropped event costs latency, never correctness, and a restart is uneventful. The `WatcherSupervisor` is the doctrine's first implementor. |
| **No mutex anywhere** | The supervisor is reached only through `&mut self` on its own pass. The prototype needed a `Mutex` because axum handlers shared its state; nothing shares this. |
| **Maximum-age settle** | The prototype's known hole: a file written faster than the window restarts it forever, and its whole directory is then never indexed again. A ceiling of six windows (60 s at the default) forces the re-list anyway. It belongs on this side because `GREATEST` only moves a deadline *forward* — the queue-level debounce has the identical hole with no escape hatch short of changing store SQL. |
| **Ignore filter before the debouncer** | `.git` is the loudest thing on a developer's disk and is in nobody's `.gitignore`; without a pre-filter every `git status` would buy a directory walk. Matched on whole path components, so `src/target_types.rs` survives. This is a noise filter, not the indexing rule — `discover` still owns what is worth scanning. |
| **Never canonicalize an event path** | `canonicalize` fails on a path that has been deleted, which is precisely when the watcher needs to reason about it. Event paths are normalised lexically; only *roots* are canonicalized, at registration, where the path is guaranteed present. |
| **Nested settled directories are pruned** | `discover` recurses, so walking both `/repo/src` and `/repo/src/deep` parses the same files twice for nothing. |
| **Never call `sync_worktree_files`** | It syncs a **whole** worktree, so handing it one subdirectory's files would reap every path outside that subdirectory. The cost is that deletions are not reaped here — see below. |
| **Enqueue through `roots.rs`'s shape only** | One `SCAN_FILE` kind, one `ScanFileJob::dedupe_key` (`scan:{worktree}:{path}` — path-shaped, so a file edited twice before the queue drains collapses to one pending scan of the latest content). No parallel queue. |
| **Overlapping roots are both watched** | `~/code` and `~/code/project` both registered means an edit is seen twice, under two worktree ids. That is duplicated bookkeeping, not duplicated work: both scans key content by blob, so the second finds everything already stored. Absorbing the covered root is a product decision, not a diff-time guess. |
| **Log the subject, never the payload** | An event line names a path. Job payloads carry indexed source text and have no business in a log. |
| **No shutdown handle** | Matches `runner::run_forever`, the daemon's only other long-lived loop: both are `tokio::spawn`ed and end with the process. A pass is idempotent, so dying mid-pass is safe — the next boot pass re-derives everything, and `requeue_running` already sweeps half-done jobs. Watchers unsubscribe by `Drop`. |

## Gotchas (the expensive ones)

- **The watcher is a hint, never a ledger.** Anything that assumes the dirty set
  is the complete list of what changed is wrong on Linux. Pair it with a walk.
- **Deletions are not reaped.** A deleted file keeps its `worktree_files` row
  until a full walk (`add` or `scan`) runs. Queued below.
- **Un-settled directories are lost on restart.** Scan jobs already enqueued are
  safe in Postgres; directories still inside their debounce window are not.
- **There is a window after `add`.** `add` walks and enqueues everything present
  at that moment; the watcher covers what changes after it is *installed*, which
  is up to one cadence (5 s) later. An edit landing in that gap is seen by
  neither. `crates/daemon/tests/watcher.rs` is deliberately ordered to show this
  rather than paper over it.
- **A perpetually-written file still delays its neighbours** by up to the
  maximum age. The ceiling bounds the damage; it does not remove it.
- **Non-UTF-8 paths are unanswered.** `serde` refuses to serialise them rather
  than transcoding, so one such filename under a watched root is a failure with
  no owner yet. Linux permits any byte but `/` and NUL.
- **inotify watch descriptors are not budgeted.** One per directory, against
  `fs.inotify.max_user_watches`. Adding `~/` as a root is currently untested.
- **The stop half of the root diff has no end-to-end proof** because the store
  has no "unregister a worktree" verb. It is pinned against the pure diff
  (`watch::tests::a_root_no_longer_registered_is_stopped`).

## Queued decisions (they join the daemon plan)

1. **Periodic full-walk backstop** — the answer to deletions, to lost pending
   directories, to the post-`add` window, and to the inotify race in general.
   The single highest-value follow-up.
2. **Root-overlap absorb policy** — accept a covering root and retire the
   covered one, re-attributing its work.
3. **inotify descriptor budgeting** — and a named error when the ceiling is hit,
   rather than a silently partial watch.
4. **Non-UTF-8 path answer at the boundary** — lossy display plus a byte-exact
   key, or refuse the path by name.
5. **`Notify` nudge handle** — only if add-latency ever becomes visible.
6. **Graceful shutdown** — one ctrl-c future wired to both long-lived loops.

## How to verify it works

```bash
docker compose up -d

# The pure core: debounce algebra, max-age, ignore rules, nesting, root diff
cargo test -p fs3-daemon --lib debounce
cargo test -p fs3-daemon --lib watch
cargo test -p fs3-daemon --lib reconcile

# End to end: real OS watcher, real runner, throwaway database
cargo test -p fs3-daemon --test watcher

# The architecture edges this added (fs3-daemon -> notify, async-trait)
cargo run -p fs3-testkit --bin fs3-arch-check
```

The end-to-end tests drive `reconcile()` by hand rather than spawning
`run_forever`, so an assertion is about *what a pass does* and never a race
against a five-second timer. The loop's own behaviour — immediate first pass,
error containment — is pinned by unit tests in `reconcile.rs`.

## Code pointers

- `crates/daemon/src/debounce.rs` — `Debouncer`, `prune_nested`, `is_ignored`,
  `normalize`. No clock, no I/O, no threads.
- `crates/daemon/src/watch.rs` — `WatcherSupervisor`, `diff_roots`. The
  `notify` shell and the only place that talks to the store.
- `crates/daemon/src/reconcile.rs` — `Reconcile`, `run_forever`. Written once,
  for every implementor.
- `crates/daemon/src/boot.rs` — the composition root; where the roster is built.
- `crates/core/src/config.rs` — `IndexingConfig::debounce_seconds`.
- `crates/testkit/arch-allowlist.toml` — the `fs3-daemon → notify` and
  `fs3-daemon → async-trait` rows.
- `pocs/daemon-shell/LEARNINGS.md` — the measured transcripts every claim here
  rests on.
