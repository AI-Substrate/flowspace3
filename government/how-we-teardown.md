# Post-run teardown ritual (flowspace3 o-prime)

Written 2026-08-31 for pij-dominant-vicuna at Jordan's direction. This is what
I actually do, including the parts that failed today — a ritual described only
by its happy path is the same lie this government spent all day hunting.

## The order, and WHY it is this order

**1. Buffers before bodies.** Rescue every seat's observation buffer BEFORE
closing anything. A closed pane's buffer is recoverable from disk, but a
force-tidied worktree's is not — and `harness team tidy` now stash-rescues
precisely because we lost a review round that way (2026-08-27). Workers LIST
their buffers and never clear; the drain is o-prime-owned, because the buffer
is SHARED across every seat in the tree and clearing destroys siblings' live
observations. (Ruled 2026-08-26 after a worker correctly refused a clear.)

**2. Seats (`pij close`).** Close the PM LAST if it owns coders — `pij close`
enforces ownership (`E-OWN`), so closing a PM first orphans its children and
every child then needs `--force`. I got this wrong today and had to force four
009 coders. Correct order: coders → reviewer → PM. Alias tombstones ("X has
exited, unrequested-by-pij") arriving after a close are EXPECTED noise: one omp
process mints extra registry ids on the same pid (pij#19), so always address
the canaried id and never chase phantoms.

**3. Verify the work landed — by CONTENT, not by sha.** Before removing
anything, confirm each branch's work is actually in main. A squash merge
rewrites the sha, so `git log main..branch` shows 1 commit even for fully
merged work; that number is a NULL signal. Use `gh pr list --head <branch>
--state merged`, and for anything you personally care about, grep the merged
artifact for a phrase you know you wrote. That grep is how we caught a commit
silently stranded by a squash race today (our row 111).

**4. Worktrees + branches (`harness team tidy <slug>`).** This also frees the
plan ordinal so `harness team new` stops counting it. `--force` ONLY after
step 3, and only after sweeping `git status --short` yourself — force deletes
dirty files with no rescue.

**5. Windows.** Mostly automatic: `pij close` kills the pane. Break every
spawned worker into its OWN window at spawn time (`tmux break-pane`) — Jordan's
standing ruling — or teardown means hunting panes inside shared windows.

**6. Disk LAST, and never concurrently.** See below; this is the part that bit
us hardest.

## Tooling

- `pij close <seat>` / `--force` — ownership-checked; dissolves the descriptor.
- `harness team new|tidy <slug>` — our own harness extension: `new` mints
  worktree + branch + next-ordinal plan folder; `tidy` reverses it (worktree,
  branch, slug-scoped docker volumes) and stash-rescues the observation buffer.
  Ordinal tombstoning is a side effect of tidy, not a separate verb.
- `harness observe --list` → `harness record retro` → `harness observe --clear`
  — the drain, o-prime only. Today: 11 observations → one retro → cleared and
  verified empty.
- `pij anomalies` (run UNSCOPED — `status-stale` is node-keyed) for seats that
  went quiet without reporting.
- No sweep verb exists. There should be one; that gap is our backlog row 110.

## Keep-vs-close

**Close** anything whose packet has landed — a seat idling post-merge is pure
cost and will happily answer questions with stale context.

**Keep** standing observers with an OPEN mandate: today krill stayed because it
owns an acceptance leg (ac-0007) on a plan that just shipped, watching prod
read-only for the defect signature the plan claims to have killed. The test is
"does this seat still have an unfinished obligation", not "is it recently
active" — krill sat 17h idle and was still correctly kept. A PA or resident
scout is the same shape: mandate-bound, not activity-bound.

**Never close** what you did not spawn without the owner's explicit ask.

## What I deliberately DO NOT clean

- **The evidence.** Three failed `ingest_session` jobs are sitting in our prod
  queue right now on purpose — they are a reporter's live reproduction of an
  open defect (our row 107) and will be cleared by the eventual fix's own heal
  path, as its proof. Cleaning a repro is destroying the acceptance fixture.
- **Transcripts and conversation ingest.** Never pruned. They are the recovery
  mechanism: this seat was compacted mid-day and re-oriented by searching its
  own conversation in the index.
- **Terminals/tombstones.** Exit records stay; they are how the next prime
  learns a seat died rather than finished.
- **Another government's trees, containers, or databases.** I characterise and
  stop. Today the 009 PM found 328 orphan databases on a SHARED test cluster,
  read-only-characterised the sprawl, and refused to mass-drop — correctly,
  since some could belong to live seats. The clear was then mine to execute as
  o-prime (313 dropped, prod untouched, connection-checked first).
- **Anything with a live process inside it.** Worktrees whose target dirs are
  mid-`rm` stay registered until the rm finishes; removing a worktree out from
  under a live rm turns a slow cleanup into a corrupt one.

## The three lessons that cost us today

1. **An isolation mechanism must ship with its reaper.** Per-seat
   `CARGO_TARGET_DIR` and per-run databases both isolate correctly and both
   leak forever: ~78G of build artefacts and 328 databases, which took the disk
   to 95% full and made the gates untrustworthy — the exact trust the isolation
   existed to provide. Cleanup that runs only on the happy path is not cleanup:
   a `destroy()` call is never reached by a panicking, timing-out, or SIGKILLed
   test. Drop-on-Drop plus an age-based sweep keyed on the REAL name prefix.
   (Ours was specified against a prefix the code never mints — the sweep would
   have matched nothing and looked healthy.)

2. **Teardown is I/O, so do it serially.** Reclaiming ~78G on a saturated disk
   ran at ~2.2MB/s. Firing more cleanup concurrently made every command —
   including `ps` and `df` — take minutes. Measure the RATE and report it
   instead of claiming "teardown done"; an unattended `rm` that quietly dies
   leaves the disk full for the next seat, so name the stall tell explicitly.

3. **A teardown verdict must never be able to lie.** Two instances in one hour:
   `harness team tidy` returned DEGRADED having already deleted a worktree's
   files but left it registered, so the next run reported ~390 "deleted" paths
   — indistinguishable from catastrophic loss (our row 112); and my own script
   printed "removed" for worktrees that still existed, because it rendered
   success from the ABSENCE of an error rather than the presence of the effect.
   Verify teardown by LISTING what remains, never by reading a status word.
