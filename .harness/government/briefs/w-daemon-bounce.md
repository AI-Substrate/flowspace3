# w-daemon-bounce — `harness daemon bounce` as a first-class harness verb

**From**: pij-instant-lynx · 2026-08-30 · Jordan ruled: "yes, make it first
class in harness thanks". Closes backlog row 82; encodes ruling
`rulings/2026-08-30-always-bounce-daemon-on-merge.md`.

## The job

A repo harness extension (sibling to `team new`/`team tidy`/`checks` under
`.harness/extensions/`) that turns the manual merge->bounce ritual into one
verb:

`harness daemon bounce [--json]`

1. **Freshness guard**: `git fetch` then REFUSE if HEAD != origin/main
   (names both SHAs and says `git pull --ff-only` first) — the stale-binary
   bounce defense. `--allow-dirty-head` overrides explicitly, stating why in
   the envelope.
2. **Build**: `cargo build --release` in the repo root; on failure, stop
   with the compiler tail (bounded, labelled — row 77 lesson).
3. **Locate the daemon**: find the running prod daemon process/pane
   (lsof on the configured port; the tmux pane is discoverable, do not
   hardcode %50 — panes change). If none is running, say so and just start
   it (that is a valid bounce from cold).
4. **Drain-restart**: SIGINT/C-c the daemon (post-#64 shutdown finishes
   in-flight only), bounded wait with progress, relaunch in the same pane
   (or print the exact command if no pane owns it).
5. **Verify**: poll /health until the auth 401 tell appears (bounded, e.g.
   120s); then print/emit version, port, elapsed, and queue counts. A
   failed verify is a LOUD failure naming what to check — never a silent
   partial bounce.
6. **Envelope**: agent-first JSON with next_action at every failure point;
   human render for the terminal.

## Proof

- A transcript of a real bounce (build -> drain -> up -> 401 tell ->
  version) in the PR.
- The refusal cases exercised: stale HEAD, build failure, verify timeout
  (fake port) — each with its named next_action.
- Extension loads via `harness doctor` extension inventory.

## Fence

IN: the new extension, its tests/receipts, one line in
`.harness/engineering-harness.md`'s signal inventory. OUT: the daemon's own
shutdown semantics (#64 owns them), release-please/versioning, CI.
Standard rules: worktree fs3-daemon-bounce, plan-ack before code, per-seat
CARGO_TARGET_DIR, never test the bounce against prod :7373 — prove the
restart mechanics against an ISOLATED daemon on an alternative port, and
the prod transcript happens as MY post-merge bounce, not from your seat.
