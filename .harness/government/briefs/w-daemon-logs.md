# Brief: w-daemon-logs — daemon log files with real rotation (Jordan ruled 2026-08-27)

**Seat**: (fill at canary). PR-era done-bar: own worktree + branch, conventional
commits, harness checks green (all seven gates incl. lock + testdb), PR into main,
report the number, never self-merge. Read AGENTS.md fully first (dogfooding +
observe duties bind you).

## Why now (the motivating incident, 2026-08-27)

The daemon's summarize lane died silently mid-run (9,411 jobs pending, zero claims).
The only copy of the panic evidence was Jordan's terminal scrollback, because the
daemon logs to STDOUT ONLY (`tracing_subscriber::fmt()` in crates/cli/src/main.rs).
Additionally, the standing Linux tester found that redirecting stdout pollutes the
file with ANSI escapes (no non-TTY detection), so even manual capture is degraded.
Observations DL-019 + cheetah's finding 5 cover this. Phase-2 self-restart (the
daemon exec()ing itself with no terminal) makes a log file mandatory, not nice.

## Deliverables

1. **Rolling file log**: the daemon writes tracing output to a log file IN ADDITION
   to stdout. Default location per-platform state dir (macOS
   `~/Library/Application Support` or `~/.local/state`-style — pick the idiomatic
   Rust answer via the `directories` crate family and document it); configurable
   under `[daemon]` (path, level). Update docs/reference/configuration.md (the
   complete option table has a drift test — it will remind you).
2. **Real rotation**: size- or day-based rotation with a retention cap (e.g. N files
   × M MB, or 7 dailies — your judgment, documented). Prefer `tracing-appender`'s
   rolling writer if it fits; if its rotation is too weak (it does not size-cap),
   say so in the code and do the minimal honest thing rather than pulling a heavy
   dependency. No unbounded disk growth EVER — that is the invariant.
3. **ANSI discipline**: the FILE layer is always ANSI-free; the stdout layer keeps
   color only when stdout is a TTY (fixes cheetah's finding 5 for redirected runs
   too). Two layers, one subscriber.
4. **Discoverability**: `flowspace3 doctor` gains a row naming the active log path
   (and flags when the file is unwritable — falls back to stdout-only, never
   crashes on a bad log path). Boot line logs the path once at startup.
5. **Panics land in the file**: a panic hook (or tracing-panic bridge) so lane/task
   panics are written to the log file, not just stderr — the motivating incident
   was precisely a panic that lived only in a terminal. Test: induce a panic in a
   spawned task in a test binary, assert it appears in the file.
6. **Tests**: rotation actually rotates (write past the cap, assert file count and
   sizes); non-TTY file is byte-clean of escapes (the tester's screen-assertion
   lesson: strip nothing, assert absence); config overrides respected; unwritable
   path degrades gracefully with a user_messages entry (the queue exists —
   crates/store/src/messages.rs — and "your logs are not being written" is a
   legitimate third producer; coordinate the key shape with the two existing
   producers, do not invent a new side-channel).

## Bounds

- Do NOT touch the summarize-lane bug itself (separately owned); this packet is
  the evidence substrate, not the fix.
- Do NOT change log CONTENT/levels beyond what the file/TTY split requires.
- The daemon lives in crates/daemon + the runtime init in crates/cli/src/main.rs;
  arch-allowlist may need a line — justify it in the PR body if so.
