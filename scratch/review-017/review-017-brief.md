# Review dispatch — plan 017 daemon-key-after-bind

You are the cross-model reviewer. Your packet is
`docs/plans/017-daemon-key-after-bind/packet-reviewer.dd.md` (rendered) —
read it first, it is the contract.

- **Sha under review: `6d6637d8eac60ec61a200338fb4b061b98040caf`** — PR #108,
  branch `017-daemon-key-after-bind`, base `main` at `689ac27`. You are already
  detached at that sha in this worktree; do not move it.
- **What this plan does:** a second daemon could bind the other loopback family
  and overwrite the shared auth key, logging every client of the first daemon out.
  This happened twice in production today. The fix: canonicalise `localhost` once,
  require a `BoundListener` proof before publishing the key, tell a 401 client when
  the key file is newer than the running daemon, and refuse the production database
  from an undesignated cwd.
- **Coder:** a GPT-5.6 seat (pij-comparative-cod). Its receipts are in
  `.harness/temp/agent/daemon-key-t*.md` and `daemon-key-closeout.md` in the
  coder's worktree `/Users/jordanknight/substrate/flowspace/fs3-daemon-key-after-bind`
  — read them, then disbelieve them per owed-list 2.
- **Deliver:** `review-017-ack.md` (numbered plan per AC, before you start),
  then `review-017-verdict.md`, plus a one-line mirror in `review-017-status.md`.
  Build the record at `docs/plans/017-daemon-key-after-bind/assets/reviews/review-017.dd.json`
  and run BOTH `ddocs build` and `ddocs validate` on it (they are different
  validators; validate from the worktree root).
- **Message me (o-prime, rs id `pij-binding-magpie`) the moment you are blocked** —
  before you write a status file. Two seats today idled for ~18 minutes because they
  wrote their blocker to a file and waited. A one-line pij send costs nothing.
- **Hard fence:** read-only on code. Tests only against
  `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`, with per-run
  scratch databases you create and drop. NEVER a daemon with the default config dir,
  never `:7373`, never `:5433`, never `~/.config/flowspace3/daemon.key` (its mtime is
  `16:57:45` and must still be that when you finish). The exclusive `harness checks`
  slot is not yours — targeted `cargo test` only. CI is green on `6d6637d`
  (run 33610456864); re-read it yourself rather than trusting me.

Acknowledge with the numbered plan, then review.
