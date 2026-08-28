# w-daemon-restart — find the daemon's tmux pane, stop it, restart it there

Jordan ask 2026-08-28: a script that can FIND the pane the daemon is running
in, stop it, and restart it in that pane. Today this is a manual o-prime
procedure (memory file names pane %50 by id — which rots the moment the
layout changes). Encode it.

## Requirement

1. A script (bin/ or scripts/, runnable standalone) — `daemon-restart` —
   that: (a) DISCOVERS the tmux pane currently running `flowspace3 daemon`
   by inspecting panes across ALL sessions (match the running command tree,
   not a hardcoded pane id); (b) stops it (C-c via send-keys, bounded wait
   for process exit, escalate to SIGTERM on timeout — never SIGKILL without
   a flag); (c) restarts the daemon IN THAT SAME PANE; (d) verifies health
   afterwards (the daemon HTTP health endpoint answering with auth) and
   reports what it did.
2. `--binary <abs path>` optional flag: relaunch with a specific binary
   (default: `flowspace3` from PATH). This is the branch-swap flow the prime
   used twice today — first light on an unmerged binary, then restore.
3. Refusals (fail-closed, named): no pane found running the daemon (say so,
   suggest how to start one); MULTIPLE candidate panes (list them, refuse to
   guess); tmux not present/daemon running outside tmux (report, do not
   attempt kill by pgrep — killing a process you did not positively identify
   is out of scope).
4. Output: human-readable lines + a final single-line summary (pane id,
   old pid, new pid, health verdict). Nonzero exit on any failure.
5. Idempotent-safe: running it when the daemon is healthy simply restarts
   it; running it twice does not double-start.

## Constraints

- Own worktree ../fs3-daemon-restart, branch w-daemon-restart off main.
- Shell or Rust — coder's choice with one line of reasoning (a script is
  fine; this is ops tooling, not product surface). If shell: shellcheck
  clean. If Rust: it does NOT join the workspace as a product crate without
  asking.
- Test story: a fake "daemon" (sleep loop with the right argv shape) in a
  scratch tmux session — prove discovery, stop, restart, refusal-on-two.
  Never test against the REAL daemon's pane.
- ABSOLUTE PATHS (DL-007/008); harness observe frictions, list never clear;
  harness checks green; PR held unmerged.
