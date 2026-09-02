# Disk space emergency — w-disk-space report

Seat: `w-disk-space` (rs). Brief: `fs3-governance/government/briefs/w-disk-space.md`.
Window: 2026-09-02 02:14Z → 03:2xZ (12:14 → 13:2x local).

## Headline

1. **OrbStack did not break itself — the host disk killed it.** `~/.orbstack/log/vmgr.log`
   at `2026-09-02T02:09:18.296317Z`:
   `block req failed: write failed @ 43483119616: Os { code: 28, kind: StorageFull }`
   → `BTRFS error (device vdb1): bdev /dev/vdb1 errs: wr 1` → `Transaction aborted (error -5)`
   → `stop requested reason=9` → `VM stopped`. The docker socket vanished because the VM
   was gone, not the other way round. It needed free host space, then `orb start` —
   which o-prime performed at 12:20 local (`orb status` = Running, `~/.orbstack/run/docker.sock`
   present). No reset, no data loss.
2. **The 128.7 GB of docker volumes is NOT today's growth.** Creation dates prove it —
   see "What did NOT cause it". It is ~10 months of accumulated sprawl and is still the
   single biggest reclaimable pool (99.97 GB).
3. **Today's ~170 GB was overwhelmingly cargo `target/` sprawl** across ~25 per-worktree
   build directories in `~/substrate` and `~/pi-hacking` — measured at **~106 GB live**
   when I started, essentially all of it written on 08-31 → 09-02.

## Ranked consumers (measured, with the command)

`du -xd1 ~` (note: macOS `du` without `-k` reports 512-byte blocks — halve the raw number).
Home total **1402 GB**.

| Size | Path | Today's growth? |
|---|---|---|
| 575 G | `~/VideoMedia` | No — Jordan personal media (since purged by Jordan) |
| 283 G | `~/Library` | Partly — contains the OrbStack image (see below) |
| 111 G | `~/github` | No |
| 86 G | `~/substrate` | **YES** — fs3 seat worktrees, ~48 GB of it `target/` |
| 86 G | `~/Parallels` | No — Jordan personal VMs |
| 45.7 G | `~/pi-hacking` | **YES** — pij seat worktrees, ~58 GB of `target/` before reaping |
| 33 G | `~/Downloads` | No |
| 24 G | `~/games` | No |

Inside `~/Library` (`du -xsh`):

| Size | Path | Note |
|---|---|---|
| **142 G** | `~/Library/Group Containers/HUAQ24HBR6.dev.orbstack/data/data.img.raw` | The OrbStack VM disk (2 TB sparse apparent, 142 G allocated). Holds all docker content. Never shrinks on its own. |
| 22 G | `~/Library/Developer/CoreSimulator` | iOS simulator runtimes/devices — regenerable, NOT deleted (needs a GO) |
| 19.4 G | `~/Library/Caches` | Diffuse; Homebrew 4.0 G, Microsoft 3.9 G, Spotify 1.6 G, playwright 1.0 G |
| 1.3 G | `~/Library/Developer/Xcode/DerivedData` | deleted |

Cargo `target/` dirs — `find ~/substrate ~/pi-hacking ~/github ~/games -maxdepth 4 -type d -name target` then `du -xsm`, ranked (state at 02:20Z, before reaping):

```
17596 MB  2026-09-02 10:41  ~/substrate/flowspace/flowspace3/target          (live: main)
 8585 MB  2026-09-01 18:23  ~/pi-hacking/pij-worktrees/s122-tmux-full-body/target
 8464 MB  2026-09-02 11:39  ~/substrate/flowspace/fs3-search-admission/target (live seat)
 8029 MB  2026-09-02 11:39  ~/substrate/flowspace/fs3-jobs-retention/target   (live seat)
 5892 MB  2026-09-02 07:16  ~/pi-hacking/pij-worktrees/s121-adopt-is-consent/target
 5636 MB  2026-08-31 19:37  ~/pi-hacking/pij-worktrees/s110-claude-delivery/target
 4653 MB  2026-09-02 12:17  ~/pi-hacking/pij-worktrees/s123-one-message-one-delivery/target (BUILDING NOW — untouched)
 4170 MB  2026-09-01 14:55  ~/pi-hacking/pij-worktrees/s117-delivery-repro/target
 4010 MB  2026-09-01 15:18  ~/pi-hacking/pij-worktrees/s118-framing/target
 3855 MB  2026-08-31 19:56  ~/pi-hacking/pij-worktrees/s113-claude-bind/target
 3642 MB  2026-08-31 13:19  ~/pi-hacking/pij-worktrees/s109-rust-ci/target
 3622 MB  2026-09-02 08:36  ~/pi-hacking/fs3-claude-inbound-accept-global/target
 3387 MB  2026-09-01 12:26  ~/pi-hacking/pij-worktrees/s116-consent-default/target
 3252 MB  2026-09-02 10:29  ~/pi-hacking/fs3-seat-session-identity/target
 3211 MB  2026-09-02 10:06  ~/pi-hacking/fs3-claude-session-start-adopt/target
 3166 MB  2026-09-02 10:26  ~/pi-hacking/fs3-seat-session-identity-u1/target
 2346 MB  2026-09-01 13:18  ~/pi-hacking/pij-worktrees/s114-ready-on-rs/target
 2237 MB  2026-09-02 12:09  ~/pi-hacking/pij/target                          (live: pij main)
 2017 MB  2026-09-01 08:34  ~/pi-hacking/pij-worktrees/s115-name-corpus/target
 1770 MB  2026-09-01 14:19  ~/pi-hacking/pij-worktrees/relander-114/target
 1629 MB  2026-08-31 12:32  ~/pi-hacking/pij-worktrees/s109-review-spawn-rpc-port/target
 1614 MB  2026-08-31 10:04  ~/pi-hacking/pij-worktrees/s108-r4/target
 1441 MB  2026-09-02 12:15  ~/pi-hacking/pij-worktrees/s108-rust-port/target
  908 MB  2026-09-02 07:51  ~/pi-hacking/fs3-omp-event-reattach/target
  885 MB  2026-09-02 12:09  ~/substrate/flowspace/fs3-fresh-db-serialise/target (live seat)
  285 MB ×3                 s111-u-observer, s111-u-gate, s110-u-adopt
──────
~106 GB total, EVERY mtime within the last 48 h.
```

Regenerable caches (`du -xsm`):

```
22.0 G  ~/Library/Developer/CoreSimulator     (not deleted — needs GO)
19.4 G  ~/Library/Caches
17.9 G  ~/.npm   (16.5 G of it ~/.npm/_cacache)
 6.0 G  ~/.cache (3.7 G uv, 0.7 G huggingface, 0.5 G puppeteer)
 2.8 G  ~/Library/pnpm
 1.8 G  ~/.cargo/registry
 1.3 G  ~/Library/Developer/Xcode/DerivedData
 0.6 G  ~/.bun/install/cache
```

APFS local snapshots: **none** — `tmutil listlocalsnapshots /` returns an empty list.
Not a factor. Purgeable space: `diskutil info /` showed no purgeable reserve worth chasing.

## What did NOT cause it (o-prime's docker hypothesis is disproved)

`docker volume ls -q | while read v; do docker volume inspect -f '{{.CreatedAt}}' $v; done | sort -r`

The **newest non-pgdata volume was created 2026-08-27**. Everything large is older:

```
2026-09-02 12:08  flowspace3_flowspace3-pgdata-test   48 MB   ← today, tiny
2026-09-02 11:42  fs3-search-admission_..-pgdata        0 B   ← today, empty
2026-09-02 08:21  fs3-embed-cap-heal_..-pgdata          0 B   ← today, empty
2026-09-02 08:21  fs3-conv-verify_..-pgdata             0 B   ← today, empty
2026-08-27 17:52  fs3-linuxtest-dockerlib             857 MB
2026-08-26 19:50  flowspace3_flowspace3-pgdata       9.96 GB  ← PROD, NEVER TOUCH
2026-08-26 15:37  fs3-cargo-target                  21.18 GB
2025-11-10 10:19  dind-var-lib-docker-0ioib8rv...   12.67 GB
2025-11-02 10:49  vscode                            11.54 GB
2025-11-23 14:54  dind-var-lib-docker-09467veb...   10.38 GB
2026-04-26 17:45  chainglass_dot_next_084-...        7.03 GB
… 40 more, all 2025-11 → 2026-04
```

So the docker volume pool did not grow today. It is a **standing 100 GB debt**, not the
day's spike. The prod-postgres WAL churn (12.67 GB in 2.2 h) and the 60 leaked test
databases did write into `data.img.raw` today and permanently inflate it — a btrfs image
never returns blocks to the host — but that mechanism accounts for tens of GB, not 170.

## What I deleted, with `df` before/after

All figures are `df -h /System/Volumes/Data` "Avail". **Caveat: another agent (o-prime)
was deleting concurrently in the same window, so the deltas below are shared, not solely
mine.** Every path deleted was a `target/` build-artefact directory or a regenerable
cache. No source tree, no git object, no database data, no `~/.claude`, no `~/.pij*`.

### Step (a) — stale build artefacts, 02:16Z → 02:19Z

Avail **60 GiB → 94 GiB (+34 GiB)**.

Selection rule: `target/` only, in worktrees whose branch is merged into `main` **or**
whose last build was ≥4 h old. Anything building right now was skipped
(`s123-one-message-one-delivery` mtime 12:17, `pij` main 12:09, `s108-rust-port` 12:15,
and all six live fs3 seats).

| Size | Path (all `…/target`) | Why safe |
|---|---|---|
| 8585 MB | `~/pi-hacking/pij-worktrees/s122-tmux-full-body` | idle 18 h |
| 4170 MB | `~/pi-hacking/pij-worktrees/s117-delivery-repro` | branch `s119/send-inbox` MERGED |
| 4010 MB | `~/pi-hacking/pij-worktrees/s118-framing` | MERGED |
| 3855 MB | `~/pi-hacking/pij-worktrees/s113-claude-bind` | MERGED |
| 3642 MB | `~/pi-hacking/pij-worktrees/s109-rust-ci` | MERGED |
| 3387 MB | `~/pi-hacking/pij-worktrees/s116-consent-default` | MERGED |
| 2346 MB | `~/pi-hacking/pij-worktrees/s114-ready-on-rs` | MERGED |
| 2017 MB | `~/pi-hacking/pij-worktrees/s115-name-corpus` | MERGED |
| 1770 MB | `~/pi-hacking/pij-worktrees/relander-114` | MERGED |
| 1629 MB | `~/pi-hacking/pij-worktrees/s109-review-spawn-rpc-port` | idle 2 d |
| 1614 MB | `~/pi-hacking/pij-worktrees/s108-r4` | detached review tree, idle 2 d |
| 908 MB | `~/pi-hacking/fs3-omp-event-reattach` | branch `124-…` MERGED |
| 285 MB | `~/pi-hacking/pij-worktrees/s111-u-observer` | MERGED |
| 285 MB | `~/pi-hacking/pij-worktrees/s111-u-gate` | idle 2 d |
| 285 MB | `~/pi-hacking/pij-worktrees/s110-u-adopt` | MERGED |

Nominal total **38.8 GB**. Several of these raced with o-prime deleting the same paths
(`rm: … No such file or directory`), so my exclusive contribution is smaller than 38.8 GB.

### Step (b) — regenerable caches, 02:41Z → 03:05Z

| Size | What | Command |
|---|---|---|
| 16.5 GB | `~/.npm/_cacache` (verified 16863 MB → 2 MB) | `npm cache clean --force` |
| 3.7 GB | `~/.cache/uv` (verified gone) | `rm -rf` |
| 1.34 GB | `~/Library/Developer/Xcode/DerivedData` (verified 0) | `rm -rf …/*` |
| 1.8 GB | `~/Library/Caches/*.ShipIt` app-updater leftovers | `rm -rf` |
| 4.3 GB | Homebrew cache (verified 4.0 GB → 42 MB) | `brew cleanup -s --prune=all` |

**~27.6 GB**, every line verified by re-measuring the path afterwards.

### Step (c) — docker volume / image / build-cache prune, 03:2xZ

Run on o-prime's explicit GO, after OrbStack was already back up (o-prime started it;
I did not start, stop, or reset OrbStack or any container).

Host `df` Avail: **689 GiB → 703 GiB (+14 GiB)**.

Docker's own accounting, before → after:

| | Before | After | Reclaimed |
|---|---|---|---|
| Local Volumes | 56 vols / 128.7 GB | 17 vols / 30.32 GB | **98.4 GB** |
| Images | 16 / 13.39 GB | 5 / 3.479 GB | **9.91 GB** |
| Build Cache | 12 / 4.967 GB | 0 / 0 B | **4.97 GB** |
| **Total inside the VM** | | | **≈113 GB** |

Selection: `docker volume ls -q --filter dangling=true` (i.e. `LINKS 0`, referenced by no
container), then `grep -v pgdata`, then a manual hold-back of four stateful volumes.
**39 volumes removed**, all confirmed by docker echoing each name:

```
chainglass_dot_next                         chainglass_node_modules
chainglass_dot_next_066-wf-real-agents      chainglass_node_modules_066-wf-real-agents
chainglass_dot_next_073-file-icons          chainglass_node_modules_073-file-icons
chainglass_dot_next_074-actaul-real-agents  chainglass_node_modules_074-actaul-real-agents
chainglass_dot_next_077-random-enhance…-2   chainglass_node_modules_077-random-enhance…-2
chainglass_dot_next_078-mobile-experience   chainglass_node_modules_078-mobile-experience
chainglass_dot_next_083-md-editor           chainglass_node_modules_083-md-editor
chainglass_dot_next_dev                     chainglass_node_modules_dev
chainglass_dot_next_plan074p4-C5ziHF        chainglass_node_modules_plan074p4-C5ziHF
dind-storage                                vscode
dind-var-lib-docker-0br8682vuckpjveetv3l7elusrqg3ve4q3k90fr2jm30p31kfqco
dind-var-lib-docker-0ioib8rvnb71skvaj9etqjddcpqq8r76sht0c7cfj8e9si04rrv7
dind-var-lib-docker-1avkd8s7dqlabfhendu2vqmv8v5lof5ho19qig8vnk6jhj57udm8
dind-var-lib-docker-1q1hicaj6knsdndlrn0utohf3pthei3ttam5umja8pd4hcm9elop
dind-var-lib-docker-1tv2n4d3t1pvgfd1dmjeiiana30i5ug3ooosedad6o3om2io95c8
dind-var-lib-docker-04th6hg8s48ula5efcbtu2l9b02f2bau01ko54dp6q5pchrlu8ts
dind-var-lib-docker-09sn6q98id94ju695q6vd9qre9hno86js5nqdm22613es2o6gl3u
dind-var-lib-docker-15fngofv6gc8fnfb2ucq8b8tuomui0e41rjbukk9ccve5lpc8hju
dind-var-lib-docker-09467vebg2b93irfvv2bece7upbq0ul3qongfg4kgsjq0bhv4124
dind-var-lib-docker-10940c8eqolt8rludqphg50l6q50iojgq57fb3mc840c8p0836q3
fs3-bin              fs3-cargo-registry     fs3-cargo-target
fs3-poc-bin          fs3-poc-cargo-registry fs3-poc-cargo-target
fs3-rustup           fs3-rustup-arm64       fs3-rustup-x64
```

Then `docker builder prune -af` (4.967 GB) and `docker image prune -af` (9.906 GB).

**Protected — never passed to `rm`.** Every volume whose name contains `pgdata`:

```
flowspace3_flowspace3-pgdata          ← PROD, 9.96 GB, still LINKS 1
flowspace3_flowspace3-pgdata-test     ← test DB, still LINKS 1
fs3-conv-verify_flowspace3-pgdata     fs3-embed-cap-heal_flowspace3-pgdata
fs3-embed-split_flowspace3-pgdata     fs3-search-admission_flowspace3-pgdata
fs3-poc-pgdata   028-server-mode_pgdata   subspace-relay_pgdata
```

**Held back deliberately** — dangling and non-pgdata, but stateful rather than cache, and
worth only 0.5 GB between them: `jk-claw_caddy_data`, `jk-claw_caddy_config` (Caddy TLS
material), `minih-otel_lgtm-data` (Grafana/Loki history), `028-server-mode_uploads`.

**The host has not yet seen most of the 113 GB, exactly as predicted.**
`data.img.raw` went 142 G → **128 G**; the other ~99 GB is now free *inside* the btrfs
filesystem but not yet returned to APFS. OrbStack reclaims it by trimming the sparse image
in the background, and materialises the rest on the next VM restart. I did not restart the
VM — OrbStack is o-prime's to operate. **Expect ~99 GB more host space to appear once
OrbStack next trims or restarts.**

Avail at time of writing: **703 GiB** — dominated by Jordan purging personal media, not by
my reaping.

## What I would delete next, on a GO

1. ~~Non-pgdata docker volumes~~ — **DONE**, see Step (c). 113 GB reclaimed inside the VM;
   ~99 GB of that is still waiting on an OrbStack trim/restart to reach the host `df`.
   **This is the one thing left worth watching:** if host free space has not risen by
   ~99 GB after OrbStack's next restart, the sparse image needs an explicit reclaim
   (OrbStack settings → disk, or `orb` restart) — an o-prime action, not mine.
2. `~/Library/Developer/CoreSimulator` — 22 GB of simulator runtimes/devices. Regenerable
   but a slow re-download; not touched without a GO.
3. `~/Library/Caches/Microsoft` 3.9 GB, `~/Library/Caches/com.spotify.client` 1.6 GB,
   `~/Library/Caches/ms-playwright` 1.0 GB.
4. `cargo sweep`/prune on the **live** seats' `target/` dirs (17.6 GB in flowspace3 main
   alone) — only when those seats are idle.

## The cause, in one line

**Per-worktree cargo `target/` directories: ~25 of them, ~106 GB, every one rebuilt from
scratch in the last 48 h — that is the ~170 GB the day ate; the docker volume pool
(128.7 GB, 100 GB reclaimable) is a separate, older 10-month debt that made the machine
fragile enough for the target sprawl to tip it over.**

Encoding, per row 110: **one shared `CARGO_TARGET_DIR` per repo** (e.g.
`export CARGO_TARGET_DIR=~/.cargo-target/<repo>` in the worktree bootstrap, or
`build.target-dir` in a repo-level `.cargo/config.toml`) collapses 25 × 4 GB of identical
dependency builds into one, and a `cargo sweep --time 3` reaper on worktree teardown
keeps it bounded. Secondary, per row 124b/110: the `dind-var-lib-docker-*` volumes leak
one per DinD run and are never reaped — 31.8 GB across 8 of them.

## Friction captured

`harness observe` entries filed (listed, never cleared — buffer is shared):
- macOS `du` without `-k` reports 512-byte blocks; two agents produced 2× divergent home
  totals from the same command before it was caught.
- Two agents reaped the same `target/` paths concurrently, producing hundreds of
  `rm: … No such file or directory` lines and unattributable `df` deltas. There is no
  lock or claim mechanism for destructive cleanup.
