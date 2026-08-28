# w-epipe — the CLI must survive its reader leaving

Evidence (tapir, 2026-08-28): `flowspace3 status | jq -e ...` — jq exited
early, flowspace3 PANICKED on the broken pipe. A CLI that panics when its
reader exits early (jq -e, head, grep -m1) fails ordinary Unix composition.

## Requirement

1. EPIPE on stdout/stderr = QUIET EXIT (conventional success-style exit for
   SIGPIPE-class death, no panic, no backtrace), for EVERY verb — fix at the
   emit/write seam, not per-verb.
2. A test that proves it: spawn the CLI against a reader that closes after
   one byte; assert no panic marker on stderr and a sane exit code.
3. No behaviour change on any other path; envelope bytes untouched (agent
   contract; 007's goldens are the tripwire and must stay green).

## Constraints

- Own worktree ../fs3-epipe, branch w-epipe off main. ABSOLUTE PATHS
  (DL-007/008); CARGO_INCREMENTAL=0; no docker compose up; never boot a
  daemon outside the sandbox recipe in w-daemon-sandbox.md (interim rule).
- Collision surface: crates/cli emit/write path — 007 u-r is rendering in the
  same crate; keep the fix at the lowest write seam and expect a rebase.
- harness checks green; done report with ASSUMPTIONS + prove-in-tree; PR held.
