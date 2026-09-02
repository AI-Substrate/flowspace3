# w-disk-space — find the 250 GB and get it back (Jordan, 2026-09-02: "get an agent on the disk space, that is insane — we had 250 GB")

## State at spawn
`/` is 96% full: 587 MiB free at 02:2xZ, then ~10 GB after o-prime deleted two build targets. OrbStack's docker socket has VANISHED (`~/.orbstack/run/docker.sock` missing) — almost certainly disk-full fallout; that takes the prod postgres (:5433) and the new test postgres (:5434) with it. The fs3 daemon still answers /health on a pooled connection. This morning at ~09:00 the disk had 173 GB free after a 94 GB clean-up (row 110). Something consumed ~170 GB in one day.

## Known consumers (measured by o-prime, do not re-derive)
- cargo target dirs: flowspace3/target 17G, fs3-fresh-db-serialise 11G, fs3-search-admission 8.2G, fs3-jobs-retention 7.8G (+5.5G duplicate per-seat dir, deleted), fs3-review-012 5G (deleted). ~45 GB live.
- ~/.cargo/registry 1.8G. /private/tmp/claude-501 520M.
- The DB investigator's `docker system df` earlier today: Local Volumes 128.8 GB (99.97 GB reclaimable), Images 13.4 GB (8.7 reclaimable), Build cache 5 GB. The prod postgres volume was 8.2 GB of data but wrote 12.67 GB of WAL in 2.2 h under test-DB churn; 60 leaked test databases at the time.
- An o-prime `du -xsh ~/*` is running in the background: `/private/tmp/claude-501/-Users-jordanknight-substrate-flowspace-flowspace3/a5a5588f-0979-439f-a1bf-ddf185a089c7/scratchpad/du-home.txt` — read it first if it has finished.

## Your job, in order
1. **Find where the 170 GB went TODAY.** Candidates, measure each with a command and a number: OrbStack's VM disk image (`~/.orbstack/` or `~/OrbStack/` — find it; a docker volume that grew, a WAL that ran away), APFS local snapshots (`tmutil listlocalsnapshots /`), purgeable space (`diskutil info /`), ~/Library/Caches, Xcode/DerivedData, other governments' worktree targets (`~/pi-hacking/*/target`, `~/substrate/*/target`, `~/games/*`), the harness/pij daemons' logs and ledgers (`~/.pij`, `~/.pij-rs`, `~/.harness`), node_modules sprawl, and anything written in the last 24 h (`find / -xdev -mmin -1440 -size +500M`). Rank by size, with mtime, and say which are TODAY's growth.
2. **Get OrbStack back**: is it running (`pgrep -fl OrbStack`, `orb status`)? If its VM died from disk-full, say so and what it needs (free space first, then `orb start`). Do NOT reset or delete OrbStack data; the prod postgres volume lives inside it.
3. **Free space in this order, reporting free-space before/after each step**: (a) build artefacts of worktrees that are NOT live seats (live: flowspace3 main, fs3-fresh-db-serialise, fs3-search-admission, fs3-jobs-retention, fs3-review-012, fs3-governance) — anything under ~/substrate or ~/pi-hacking whose worktree is gone or whose branch is merged; (b) caches that regenerate (~/Library/Caches/*, cargo registry cache, npm cache, Xcode DerivedData, pip); (c) `docker system prune` / volume reclaim ONLY after OrbStack is back and ONLY for volumes not named flowspace3-pgdata / flowspace3-pgdata-test (list before removing; the 100 GB "reclaimable" is the target); (d) APFS snapshots via `tmutil deletelocalsnapshots` if they hold the space. NEVER: source trees, git objects, anything under a live seat's worktree except its target/, prod database data, ~/.claude, ~/.pij*.
4. **Encode the cause**: if it is build-artefact sprawl, write the one-line fix (a shared CARGO_TARGET_DIR per repo, or `cargo sweep`/a reaper — row 110); if it is docker volumes from test DBs, that is row 124b/110; if it is a runaway WAL, it is row 141. Say which, with the number.

## Report
`.harness/temp/agent/disk-space-report.md` in the main clone: ranked consumers with commands, what you deleted (with before/after `df`), what you would delete next with a GO, and the one-line cause. Send `pij send pij-binding-magpie <path>` at each step that frees >20 GB and when OrbStack is back.

## Channel and rules
rs seat; `pij send pij-binding-magpie '<line>'` lands as a turn. Never `pij adopt`. Never touch :7373. Every deletion you make is named in the report with its size. Capture friction with `harness observe`; list, never clear.
