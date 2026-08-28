# First light — the runbook for tk-a302

**Status**: procedure, written before composition so the proof is designed rather
than improvised. The TRANSCRIPT of an actual run lands beside this file as
`first-light-transcript.md` and is the exit evidence ac-0001/0003/0004 cite.
Nothing in this file is evidence of anything; it is the list of what must be
shown.

## Preconditions

1. All three units merged in order u-w → u-r → u-t, with the tui's event source
   repointed from its recorded fixture to the live stream (the one named
   function in `docs/services/tui.md`), rehearsed BEFORE it is relied on.
2. `harness checks` green on the composed branch.
3. The shared daemon on :7373 is the one being watched; nothing here boots a
   daemon, so DL-004's sandbox rule does not apply to this run. If any step
   turns out to need a private daemon, it gets a minted database, an empty
   `FS3_CONFIG_DIR`, a unique port, and a verified `embedder=fake` boot line.

## The session, in order

Everything below happens in ONE terminal session, captured whole. A transcript
assembled from several runs is not a transcript.

| # | what is shown | how | proves |
|---|---|---|---|
| 1 | human output by default | `flowspace3 status`, `search "<real question>"`, `tree`, `get <address from the search>` at a real TTY | ac-0001 (the tty leg) |
| 2 | the same invocations piped | `flowspace3 status \| head`, `search … \| jq .command` | ac-0001 (the pipe leg) + the property v1 had and must not lose |
| 3 | flags override both ways | `flowspace3 status --json` at the TTY, `flowspace3 status --human \| cat` | ac-0001 (the two flag legs) |
| 4 | the byte check | run the covered verbs piped against the goldens' fixtures and diff — or simply `cargo test -p fs3-cli --test envelope_goldens` in the same session, with `git status` showing the goldens unmodified | ac-0002 |
| 5 | **the agent check (r2)** | in THIS pij seat — a tmux PTY, which looks exactly like a person to a terminal probe — run `flowspace3 status` and show it produces JSON because `FS3_OUTPUT=json` is set for the fleet; then unset it and show the same command renders, so the override is what is doing the work | r2 retired, not assumed |
| 6 | the stream is real | `flowspace3 status --watch` in one pane while `flowspace3 add <a real path>` runs in another | ac-0004 |
| 7 | two watchers agree | a second `status --watch` attached at the same time, both showing the same events | ac-0004 (multi-subscriber) |
| 8 | add shows progress | `flowspace3 add` on a repository large enough to take seconds, at a TTY, showing the meter moving with real file counts and a real current path — and the same command piped showing NO meter and an unchanged envelope | Jordan's ruling + ac-0002 |
| 9 | the tui, live | `flowspace3 tui` against the live daemon while a scan runs: roots, queue with history, activity from the real stream (no mock tag anywhere), in-pane search going results-dominant | ac-0003 |
| 10 | the tui degrades | with the daemon stopped or unreachable: the tui says so in words, stays responsive, retries, and recovers when it returns | ac-0003's honesty half |

## What would make this run a failure rather than a demo

- Any golden modified in the branch.
- A meter on stdout, or any byte on stdout that was not there before, outside
  `status --watch`.
- The activity pane showing anything the daemon did not actually emit.
- A number on screen that is stale without saying so.
- The tui leaving the terminal in raw mode on exit or panic.

Each of those is a stop-and-report to prime, not a note in the transcript.
