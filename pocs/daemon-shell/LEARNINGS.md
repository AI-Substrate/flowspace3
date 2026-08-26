# LEARNINGS — what the real fs3 daemon needs to know

Written from a working prototype (`pocs/daemon-shell/`) driven by hand on macOS
15 / Apple M4 Max and re-run in full on Linux (`rust:1-bookworm`, aarch64,
inotify). Every number below is measured, and the transcript it came from is
included. Where something is reasoned rather than observed it says so.

The prototype exists because of the ruling
[`2026-08-26-daemon-native-on-host.md`](../../.harness/government/rulings/2026-08-26-daemon-native-on-host.md):
the daemon runs natively on mac, linux and windows, so file watching and
runtime-addable roots are ours to get right.

---

## 1. The finding that changes the design

**On Linux, files written into a directory the instant it is created are never
reported.** The first full Linux run failed exactly one test:

```
test a_hundred_file_burst_coalesces_and_settles_together ... FAILED
dirty set never reached 100 entries; last was {"count":98, "dirty":[
  {"path":"/tmp/.tmpD9tLo3/burst", ...},
  {"path":"/tmp/.tmpD9tLo3/burst/f003.txt", ...},   <- f000, f001, f002 absent
```

`RecursiveMode::Recursive` on inotify is a lie of convenience: inotify has no
recursive mode, so `notify` walks the tree and adds **one watch per directory**.
A directory created a moment ago is not yet watched, and the files created in
that window produce no events at all. macOS does not have this problem — one
FSEvents stream covers a whole subtree, so the same code reported all 100 files
plus the directory (101 entries) on the same commit.

Consequences for the real daemon, in order of importance:

1. **A dirty DIRECTORY must mean "re-list that directory", not "re-read that
   path".** The directory event is the only signal that survives the race, and
   it does survive: it is reported on both backends. This is now pinned by
   `a_directory_created_and_written_in_one_breath_still_reports_the_directory`.
2. **Event streams are not a source of truth, they are a hint to rescan.** Any
   design where the dirty set is the complete list of what changed is wrong on
   Linux. Pair the watcher with a periodic or triggered walk.
3. **`git checkout` / `git clone` / `npm install` are exactly this pattern** —
   create a directory tree and fill it faster than any watcher can subscribe.
   The common case is the racy case.

## 2. `notify` backends, per OS

| | macOS | Linux | Windows |
| --- | --- | --- | --- |
| backend | FSEvents (`macos_fsevent`, the default feature) | inotify | ReadDirectoryChangesW |
| recursion | native, one stream per root | **emulated**: one watch descriptor per directory | native, one call per root |
| new-subdir race | none observed | **loses per-file events** (§1) | not tested — same native-recursion shape as macOS *(inference)* |
| cost of a root | one stream | O(directories) kernel watches, against `fs.inotify.max_user_watches` | one handle |
| verified here | full suite, plus hand-driven 10s-debounce session | full suite in `rust:1-bookworm` | compiles, `--all-targets` (`cargo check --target x86_64-pc-windows-msvc`); **not run** |

Windows honesty: this host has no Windows to run on. Everything — library,
binary and the whole test suite — compiles clean for `x86_64-pc-windows-msvc`,
so the code is portable in the sense a compiler can prove and in no other
sense. Anything about Windows *behaviour* in this document is inference and is
marked as such. Running the suite on Windows is the obvious next experiment,
and §1 is the reason it matters: `ReadDirectoryChangesW` is natively recursive
like FSEvents, so the prediction is "no new-subdir race" — a prediction, not a
result.

### Event counts are not edit counts

One `echo hello > one.txt` produced **3** `notify` events. In the 100-file
burst, 332 raw events collapsed to 100 dirty paths, and individual files
produced up to 5. Budget for ~3–5× amplification per touched file: it is why
the debouncer keys on path and counts events rather than queueing them.

### Rename is two paths, not one

```
mv to-rename.txt renamed.txt
→ dirty: renamed.txt (events=1)      <- the new name
         to-rename.txt (events=4)    <- the old name, which no longer exists
```

`notify` 8 does offer paired rename events (`EventKind::Modify(ModifyKind::Name(RenameMode::Both))`
with two paths and a tracking cookie), but the prototype flattens every event to
its paths deliberately, to see what the naive reading costs. It costs this: the
dirty set contains a path that is gone. **Every consumer of the dirty set must
tolerate a path that no longer exists** — `rm to-delete.txt` produces the same
shape (4 events, then a dirty entry for a deleted file). Do not `canonicalize`
on the event path; it fails precisely on deletes. The prototype uses lexical
`normalize` for event paths and `canonicalize` only for roots, which is a split
worth keeping.

### Paths are not strings

`core::tests::a_non_utf8_path_cannot_be_serialized` proves that one file with a
non-UTF-8 name under a watched root turns `GET /dirty` into a serialization
error **for the entire set** — serde's `Path` impl refuses rather than
transcoding. Linux permits any byte except `/` and NUL in a filename. The real
daemon must choose: lossy display plus a byte-exact key, or refuse the path with
a named error. Silently inheriting serde's behaviour means one weird filename
can stop the whole index.

## 3. Debounce design that survived contact

The whole of it is `src/core.rs`: pure functions over values, no clock, no I/O,
16 unit tests that run in microseconds. Everything time-dependent takes
`now_ms: u64` — milliseconds on one monotonic clock the shell owns
(`Instant::elapsed`, never `SystemTime`, so an NTP step cannot make a path
settle early). This split paid for itself immediately: the algebra of coalescing,
settling, starvation and root removal is tested without a filesystem, and the
integration tests only have to prove that `notify` says what we think it says.

**Coalesce per path, restart the window on every event.** Measured, with the
fs3 default 10s window and a 250ms sweep:

| scenario | measurement |
| --- | --- |
| single file write → visible in `/dirty` | 10 462 ms wall clock (write → first non-empty poll) |
| the same, from the daemon's own clock | `last_event_ms 30561 → settled_at_ms 40752` = **10 191 ms**, i.e. 191 ms of sweep overshoot |
| 100-file burst, files written in | 88 ms |
| burst spread as the watcher saw it | 41 ms (`first_event_ms 64900 → 64941`) |
| burst → all 100 dirty | 10 250 ms after the first write; max overshoot **101 ms** |
| raw events folded | 332 → 100 entries (max 5 events for one path) |

Overshoot is bounded by the sweep interval and nothing else — a 250 ms sweep
gives 0–250 ms of lag, and the measurements (60–246 ms) sit exactly in that
band. The sweep is separate from the event pump on purpose: a flood of events
cannot delay a settle decision, and a slow settle cannot back up the channel.

**The one thing this design gets wrong: a perpetually busy file never settles.**
`core::tests::a_continuous_stream_never_settles` states it as a test rather than
a comment — a file written every 500 ms under a 1 s debounce stays pending
forever. A log file inside a watched repo is invisible to the daemon, and so is
everything about it. The real daemon needs a **maximum age**: settle anyway once
`now - first_event_ms` exceeds some ceiling, even if the file is still moving.
The prototype has no such escape hatch, deliberately, so the gap is visible.

**Unbounded channel, on purpose.** `notify` calls its callback on its own
thread, which cannot await. A bounded channel would have to block that thread —
and a blocked watcher thread is how you make the OS drop events on macOS and
Windows — or drop events itself. Unbounded turns backpressure into memory
pressure, where at least it is visible; and the debouncer collapses a burst to
one entry per path within a sweep. Under the 100-file burst the channel never
had more than a few hundred entries. *(Not tested at repository-clone scale;
that is the next experiment.)*

**Ignore rules are cheap and worth a lot.** A hardcoded component match on
`.git`, `target`, `node_modules`:

```
60 writes into .git/target/node_modules + 1 real edit
→ total_events 542, ignored_events 198, pending 1
→ /dirty: {"count":1, "paths":["real.rs"]}
```

198 of 201 new events dropped. Matching is on whole path components, not
substrings, so `src/target_types.rs` survives — a substring implementation would
silently blind the daemon to real source files. The real daemon wants gitignore
semantics, but the component rule is the right *shape*: cheap, per-event, before
the debouncer.

Case sensitivity has no correct `cfg!`: it is a property of the volume, not the
OS (APFS can be either). The prototype matches ASCII-case-insensitively
everywhere, which over-matches on Linux, because the alternative under-matches
on the case-insensitive volumes mac and Windows ship by default. The real daemon
should ask the filesystem.

## 4. Watched-root lifecycle

**Canonicalise at add time, and report what you canonicalised to.** On macOS
`/tmp` and `/var` are symlinks into `/private`, and FSEvents reports the resolved
form: skip `canonicalize` and every event looks out-of-root. On Windows the
result carries the `\\?\` extended-length prefix, which compares fine but is
ugly on the wire. `POST /watch` therefore echoes the canonical root back, and
the CLI should treat that — not what it sent — as the root's identity.

**Refuse overlapping roots.** Two recursive watches over the same file mean the
same edit lands twice under two root attributions, and a path-keyed dirty set
then flip-flops over which root owns it. Refusing is one line to explain;
merging is a lifetime of "which root did this come from". Measured:

```
POST /watch <root>          201 {"root":"…/scratch"}
POST /watch <root>          409 {"conflict":"duplicate","with":"…/scratch"}
POST /watch <root>/inner    409 {"conflict":"covered_by","with":"…/scratch",
                                 "error":"already covered by the watched root … —
                                          overlapping recursive watches would report every edit twice"}
POST /watch <root>/plain.txt        400 {"error":"… is not a directory"}
POST /watch <root>/ghost            400 {"error":"resolving …/ghost: No such file or directory (os error 2)"}
```

The reverse direction (`Covers`) matters too: adding a parent of an existing
root has to be refused, or you get the same double-attribution from the other
side. `nesting_conflict` is a pure function with the sibling-prefix trap pinned
by a test — `/a/bc` starts with the *text* `/a/b` but is not inside the
*directory* `/a/b`.

Note what this refusal costs, because the real daemon may not be able to pay it:
`flowspace3 add ~/code` after `flowspace3 add ~/code/project` is a reasonable
thing for a user to want. The choices are (a) refuse, as here; (b) accept and
transparently drop the covered root, re-attributing its pending work; (c) allow
overlap and make the dirty set key `(root, path)` instead of `path`. (b) is
probably right for fs3, and it is a small change: `watch` already re-checks
under the lock.

**Removal is ownership, not a call.** Dropping the `RecommendedWatcher` is the
unsubscribe; there is no `unwatch` bookkeeping to get wrong. What does need a
decision is in-flight work, and the prototype makes it explicit — removing a
root discards its pending and dirty paths:

```
write inflight.txt; wait 2s      → {"total_pending":1,"total_dirty":0}
DELETE /watch <root>             → {"root":"…","was_watching":true}
                                   {"roots":[],"total_pending":0,"total_dirty":0}
… 11s later (well past the window) → {"count":0,"pending":0}
DELETE /watch <root>  (again)    → 404 {"error":"… is not watched"}
log: unwatched root=… dropped_pending=1 dropped_dirty=0
```

Letting them settle after removal would hand the consumer work for a tree it no
longer watches. **Removal must also cope with a root that has been deleted from
disk** — `canonicalize` fails there, so `unwatch` falls back to lexical
normalisation. A daemon that only canonicalises cannot forget a directory the
user just `rm -rf`'d, which is precisely when they want it forgotten.

Runtime add-after-remove works and per-root attribution stays correct:

```
POST /watch <scratch>  201 ; POST /watch <second>  201
echo y > second/only-here.txt
→ roots: [{"path":"scratch","events":0,"pending":0,"dirty":0},
          {"path":"second","events":3,"pending":0,"dirty":1}]
→ /dirty: {"count":1,"paths":["second/only-here.txt"]}
```

One watcher per root (rather than one shared watcher with several `watch()`
calls) is what makes that attribution unambiguous and removal a drop. The cost
is per-root kernel resources — trivial on FSEvents, real on inotify at many
roots. *(Not measured; the inotify watch-descriptor ceiling is the number to
check before shipping.)*

## 5. HTTP shape

**`GET /dirty` does not consume.** A `GET` that empties what it reports is a
`GET` you cannot retry: every consumer that crashes between reading and
acknowledging loses work permanently. Reading and taking are split — `GET
/dirty` is idempotent, `DELETE /dirty` is the acknowledgement — which makes
delivery **at-least-once** and puts the choice in the consumer's hands. For a
queue that feeds an expensive re-scan, at-least-once is the right default:
re-scanning a clean file is cheap, missing a dirty one is not.

**`409` is not `400`.** An overlapping root is a well-formed request that the
*set* refuses; a non-existent path is the caller's mistake. Different codes, and
the body names the root it collided with so the CLI can say something useful.

**Loopback is a startup failure, not a warning.** Same rule as the real daemon
(PRD req 17 / AC-0005): the surface is unauthenticated and fronts an index of
every repo on the machine. `--bind` accepts only loopback and refuses at boot.

**`/status` is the diagnostic that mattered.** Per-root `events` /
`ignored_events` / `pending` / `dirty` is what turned "is the watcher working?"
into a five-second answer during every experiment above. Ship it.

## 6. Cross-platform discipline that actually held

There is **no `#[cfg(unix)]` or `#[cfg(windows)]` in `src/`**. The only
platform-conditional code in the whole prototype is one unit test, which is
conditional because the *bad value* (a non-UTF-8 path) can only be constructed
on unix. The rules that got us there:

- `tokio::signal::ctrl_c` and nothing else. It maps to SIGINT on unix and to the
  console control handler on Windows. `SIGTERM`/`SIGHUP` handling would be
  unix-only — the real daemon will want it, and it must be isolated behind
  `#[cfg(unix)]` with a documented Windows equivalent (service stop control).
- TCP on loopback, never a unix socket. Windows has AF_UNIX now, but tooling
  around it is uneven.
- `Path`/`PathBuf` and `Component` matching; no string splitting on `/`.
- `Instant` for elapsed time, `SystemTime` never.

## 7. What I would do differently in the real daemon

1. **Treat the watcher as a hint, not a ledger** (§1). Dirty directory ⇒ re-list.
   Add a periodic full walk as the backstop, because the watcher WILL miss things.
2. **Persist the dirty set.** It is in memory here, so a crash loses every
   pending path — and the daemon then has no idea what it missed. fs3 already has
   a queue table; the dirty set belongs there, which also makes at-least-once
   delivery survive a restart.
3. **Add a maximum age to the debounce** (§3) so busy files cannot starve.
4. **Decide overlap policy deliberately** (§4). Refusal is the honest prototype
   answer; "absorb the covered root" is probably the right product answer.
5. **Budget inotify watch descriptors** before someone adds `~/` as a root, and
   surface the failure as a named error rather than a silently partial watch.
6. **Answer the non-UTF-8 path question once**, at the boundary, rather than
   letting serde answer it with a 500.
7. **Keep the core/shell split.** It is why the interesting behaviour above is
   pinned by 16 microsecond-fast tests instead of a suite of `sleep` calls, and
   it is the one piece of this prototype I would lift verbatim.

---

## Appendix — how it was proved

```
macOS (host)     cargo test              26 passed  (16 core + 10 e2e)
                 cargo clippy --all-targets -- -D warnings    clean
                 cargo fmt --all -- --check                   clean
                 hand-driven session, release binary, --debounce-ms 10000,
                 transcripts quoted throughout §3 and §4
Linux (docker)   rust:1-bookworm, aarch64, inotify
                 cargo test              26 passed
Windows          cargo check --all-targets --target x86_64-pc-windows-msvc  clean
                 (lib, binary AND tests compile; nothing was RUN — no Windows host)
workspace        cargo run -p fs3-testkit --bin fs3-arch-check
                 → "ok - 8 crates, 63 direct edges, 0 violations"
                 (8 because a sibling added crates/git; this prototype is NOT a
                  workspace member and never appears in that count)
```

Two pieces of environment friction were recorded as harness observations rather
than worked around silently: `DL-006` (plain `cargo clippy`/`cargo fmt` resolve
to rustup shims for a toolchain without those components — since fixed at the
repo root by a `rust-toolchain.toml` pinning `stable` with both components) and
`DL-007` (cross-compiling with a second toolchain into the same `target/`
corrupts host artifacts and produces nonsense errors in your own source).
