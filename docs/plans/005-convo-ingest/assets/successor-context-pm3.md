# Successor context — PM seat 3 (predecessor pij-traditional-piranha died 2026-08-28T01:07Z)

READ FIRST: assets/successor-context-pm2.md (still accurate for rulings), then
this delta. Cause of death: the machine's disk hit 100% full (~01:04-01:07Z
sweep also killed three of your four coders); disk is being cleaned and is
workable now (~24G free) but WATCH IT — build failures surfacing as
`rustc-LLVM ERROR: IO failure on output stream` mean disk, not code.

## State you inherit (verify with git, don't trust)
- Phase 1: FROZEN, committed (a3bbfd2 seam, f32b45c ddoc close-out), accepted
  by prime. Gate was green. Phase ph-d8ff checked with receipts.
- Fan-out: packets committed at e6f5b44 (packet-coder-{u1a,u1b,u1d,u2}.dd.md).
- Piranha's additional rulings on record (in its acks/messages, honored):
  payload policy moved ONCE to core (prime-conditioned), no CursorStore trait,
  u2 done-bars amended in the packet ddoc.
- ADVISORY input from prime for the fleet: /Users/jordanknight/substrate/
  flowspace/flowspace3/scratch/harness-telemetry-reader-lessons.md (vendor into
  assets/inputs/ if distributed — refs must resolve where seats live).

## Coder status
- u2 (pij-appalling-slug): ALIVE and DONE — full report delivered to prime
  (branch 005-convo-u2, commits 5f36dc7/016fbff, gate green, mutation-checked;
  two declared deviations: pg test file outside literal fence [sound], no
  crate-root re-export [optional composer edit, in its recipe]). Its
  condition-2 note: core/daemon briefly have TWO OUTPUT_HEAD_BYTES definitions
  until the composer applies recipe step 2 (delete the daemon's, delegate).
  Slug holds for composition questions — message it that you own 005 now.
- u1a (worm), u1b (salmon), u1d (limpet): DEAD in the disk sweep. Worktrees
  survive: ../fs3-convo-u1a (barely started), ../fs3-convo-u1b, ../fs3-convo-u1d
  (partial; their target/ dirs were deleted for space — sources intact).
  Inventory each (git status + any commits), then spawn SUCCESSOR coders with
  the same packets + a short delta note each (what the predecessor left).
  NOTE their deaths may have struck mid-build: treat half-written files with
  suspicion, prefer committed state.
- Their observation buffers: u2's rescued to main; u1a/u1b/u1d had none yet
  worth rescuing (verify .harness/temp in each worktree before any removal).

## Your first moves
1. Canary to prime; ack with inventory verdict (worktrees + u2's report) +
   numbered plan.
2. Message slug that you own 005; rule its two deviations formally.
3. Respawn u1a/u1b/u1d successors (same packets, delta notes; spawn them with
   CARGO_INCREMENTAL=0 suggested and the disk-error signature named).
4. Wave 1 completion -> phase 3 composition per the impl-guide (you compose).
