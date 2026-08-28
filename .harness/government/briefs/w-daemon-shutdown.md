# w-daemon-shutdown — one coherent shutdown & sandbox-hygiene story

Pulls forward backlog rows 38, 47, 50 (and the row-53 bind-order fix, same
file, same review). Three seats paid row 38's manual-drop tax in ONE DAY
(knobbler, chickadee, kite); the o-prime paid row 47's stall twice; row 50's
ambient leak was a hard-fail only by luck. These are four defects but one
story: the daemon's promises at its edges (boot and exit) are not proven.

## The four fixes

1. **Sandbox drops its minted DB on EVERY exit path** (row 38): today the
   drop is wired to Ctrl-C only; SIGTERM — how every supervisor and agent
   stops daemons — exits 1 and leaks the database silently. Signal handler
   covers SIGTERM+SIGINT; the process states on exit whether it dropped or
   left the DB; if a drop cannot be guaranteed, printing the DB name at exit
   is the floor (a named leak is recoverable; a silent one became a 6,520-job
   landmine once already). Manual-cleanup fallback for docs: host psql may be
   absent — `docker exec flowspace3-db psql ...` is the proven form.
2. **Shutdown stops DEQUEUEING at the signal** (row 47): C-c today closes
   the listener then keeps draining the whole enrichment queue (measured:
   thousands of jobs, restart stalled until operator SIGTERM; with a large
   backlog a polite restart could take hours). Post-signal: finish IN-FLIGHT
   jobs only, log "draining N in-flight" so progress is visibly bounded.
3. **Sandbox IGNORES ambient config entirely** (row 50): it forces top-level
   embedder/summarizer to fake but ambient per-surface selections (e.g.
   [agent] active=azure-luna) still reach wiring — hard-fail today, quiet
   REAL-provider spend in the unlucky shape, the precise incident the verb
   exists to prevent. It already mints its own config dir: point the loader
   at it and nothing else. And the READY LINE PRINTS ONLY AFTER wiring is
   proven (tenet 14 — the fleet is ruled to trust that line).
4. **Key publish moves AFTER bind** (row 53, cross-government find): today
   auth::generate publishes daemon.key BEFORE the listener binds (boot.rs:82),
   so a second daemon that loses the port race still clobbers the winner's
   key — every client 401s until restart. Fix order: stage → bind → atomic
   rename. Verify restart-into-existing-key perms while there (mode applies
   on create; our NamedTempFile path is believed safe — prove it in a test).

## Proof requirements

- Each fix carries a test that fails without it; the SIGTERM drop is proven
  with a real supervised TERM, not a unit stub.
- Rows 38/47: prove with the runner busy (seeded queue) — shutdown-when-idle
  proves nothing.
- Row 50: a config with ambient per-surface selections present must sandbox
  cleanly with fakes; mutation = re-point the loader at ambient and watch it
  fail.
- Row 53: two daemons racing one port — the loser must not disturb the
  winner's key; winner's clients stay authenticated throughout.
- Tenet 15: run the composed artifact — one sandbox session exercising boot
  line, SIGTERM, DB drop, and key integrity in sequence.

## Constraints

- crates/daemon (boot.rs, auth.rs, runner shutdown) — coordinate nothing:
  single coder, own worktree (fs3-w-daemon-shutdown), no other seat touches
  these files today. Standard packet rules apply (absolute paths, sandbox
  testing, no prod 7373, harness commit once, no amend).
