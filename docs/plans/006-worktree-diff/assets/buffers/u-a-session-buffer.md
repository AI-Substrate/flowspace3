- id: DL-001
  kind: difficulty
  description: "harness boot --json exceeded the 120-second command timeout before packet work began"
  severity: degrading
  workaround: "retry with a longer bounded timeout"
  suggested_encoding: "boot should emit progress or a bounded timeout verdict naming the slow check"
  fp: 539236eb6d60
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:06:27.104Z"
- id: CONF-001
  kind: confusion
  description: "flowspace3 semantic search for offline Reconcile supervisor fake-test patterns returned no visible hits"
  severity: degrading
  workaround: "inspect exact known reconcile implementors and tests"
  suggested_encoding: "search should return an explicit empty-results envelope through compressed shell output"
  fp: 34203922197b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:06:51.759Z"
- id: DL-002
  kind: difficulty
  description: "Rust language-server reference lookup was unavailable for IndexingConfig; exact repository search was required"
  severity: degrading
  workaround: "use exact identifier search and update every struct literal"
  suggested_encoding: "boot should expose or provision Rust LSP availability before symbol edits"
  fp: 9d609bacecc7
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:07:08.337Z"
- id: CONF-002
  kind: confusion
  description: "flowspace3 search query returned no visible hits: How are Reconcile supervisors tested with hand-written fakes and tick cadence without a real database or network?"
  severity: degrading
  workaround: "read known Reconcile implementations after native LSP was unavailable"
  suggested_encoding: "retain explicit empty-result envelopes and improve shape-query ranking"
  fp: a95341f689ca
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:11:59.404Z"
- id: CONF-003
  kind: confusion
  description: "packet-supplied flowspace3 --daemon-url URL command order is rejected; the flag belongs after the subcommand"
  severity: degrading
  workaround: "probe wrapper emits flowspace3 command --daemon-url URL arguments"
  suggested_encoding: "daemon-url help and recipes should show its subcommand-local position"
  fp: 3562c3563359
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:13:25.156Z"
- id: CONF-004
  kind: confusion
  description: "Exact rejected invocation: target/debug/flowspace3 --daemon-url http://127.0.0.1:7383 add /Users/jordanknight/substrate/flowspace/flowspace3; clap says add --daemon-url exists"
  severity: degrading
  workaround: "use flowspace3 add --daemon-url URL PATH"
  suggested_encoding: "accept daemon-url globally or make command help and recipes expose subcommand-local placement"
  fp: 9eece1d86ab1
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:15:31.233Z"
- id: DL-003
  kind: difficulty
  description: "The prescribed three daemon overrides were insufficient isolation: flowspace3_test already contained fleet roots and 6500+ jobs, while ambient provider config made the isolated daemon drain them with non-fake providers"
  severity: blocking
  workaround: "stopped daemon/probe; use a unique throwaway database plus an empty FS3_CONFIG_DIR so provider defaults stay fake"
  suggested_encoding: "probe-daemon recipe must isolate database identity and config/provider selection, not only database URL, port, and log directory"
  fp: d5a18e4fbbd8
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:16:08.055Z"
- id: CONF-005
  kind: confusion
  description: "flowspace3 search with --path crates/daemon/src/worktrees.rs returned zero results, while removing the path filter returned git_worktrees and its tests from that exact file"
  severity: degrading
  workaround: "widen query without path filter"
  suggested_encoding: "search next_action should explain path glob matching or exact file paths should match themselves"
  fp: c2dbdc8c3ffe
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:22:11.945Z"
