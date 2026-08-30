# daemon — safe flowspace3 daemon lifecycle

`harness daemon bounce` replaces the merge-to-bounce ritual with one guarded,
auditable command. It is operational: it builds and may stop a running daemon.

## What it computes deterministically

1. Fetches `origin/main`, compares exact `HEAD` and `origin/main` SHAs, and
   refuses before build when they differ. `--allow-dirty-head` is the explicit
   escape hatch for isolated branch proof; the envelope records that override.
2. Runs `cargo build --release` from the repository root. A failure leaves the
   daemon untouched and retains separately labelled, bounded stdout/stderr.
3. Resolves the effective daemon URL, finds its listener with `lsof`, and maps
   that PID to its owning tmux pane by process ancestry. Pane IDs are never
   hardcoded.
4. Sends Ctrl-C to stop dequeueing and drain in-flight work, waits boundedly for
   the listener to disappear, and relaunches the new release binary in the same
   pane. A cold bounce creates a daemon pane. A listener with no discoverable
   pane is not touched; the failure returns the exact launch command instead of
   pretending a restart occurred.
5. Polls unauthenticated `GET /health` until it returns both HTTP 401 and
   `FS3-E-DAEMON-UNAUTHORIZED`, then uses the daemon's current key to collect
   authenticated health and status. Success reports version, port, elapsed
   time, and queue counts.

Every failure is loud, stage-labelled, and carries `next_action`. Use `--json`
for the stable agent envelope; terminal output is the harness core's human
rendering of the same result.

## What is expected back from you

- Run from a checkout whose `HEAD` is `origin/main`. Do not normalize
  `--allow-dirty-head`; it exists for deliberate isolated proof, not routine
  production use.
- Read a refusal before acting. In particular, never kill a listener whose tmux
  owner was not proven.
- For development proof, set `FS3_CONFIG_DIR` to disposable configuration and
  use a non-production port. Never exercise this packet against production
  `:7373`; the post-merge operator owns that transcript.
- A successful 401 tell proves the restarted daemon is answering and enforcing
  authentication. The following authenticated reports prove the current key,
  version, and queue surface agree.
