- id: CONF-001
  kind: confusion
  description: "flowspace3 semantic search for CLI subcommand wiring and DaemonClient status/search returned unrelated envelope-error material and warned this worktree was unregistered, leaving the requested ownership unclear"
  severity: degrading
  workaround: "used exact-identifier lookup only after the required semantic search"
  suggested_encoding: "rank current-worktree CLI symbols or make the cross-checkout scope warning actionable without displacing requested hits"
  fp: 86a43757dcbe
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:27:35.486Z"
- id: DL-001
  kind: difficulty
  description: "live flowspace3 tui smoke reached the shared daemon but every DaemonClient snapshot/search received FS3-E-DAEMON-UNAUTHORIZED because the newer shared daemon requires a boot key this frozen branch client does not present; terminal failure path remained responsive and restored cleanly"
  severity: degrading
  workaround: "verified honest stale/search-failed UI and will exercise success with an isolated fake HTTP surface rather than booting a daemon"
  suggested_encoding: "make shared-daemon auth/version skew visible in harness boot or provide an authenticated smoke endpoint for frozen plan worktrees"
  fp: 6af62789fb64
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:44:21.106Z"
- id: DL-002
  kind: difficulty
  description: "harness checks launched fs3-cli render_output from sibling worktree /Users/jordanknight/substrate/flowspace/fs3-hi-u-r while invoked with cwd /Users/jordanknight/substrate/flowspace/fs3-hi-u-t, so its red verdict does not describe this seat tree"
  severity: blocking
  workaround: "verify worktree roots with git/cargo metadata, report to PM, then use direct mandated cargo commands only if the gate cannot be made worktree-local"
  suggested_encoding: "make harness checks assert and print cwd/workspace_root before running, and refuse cross-worktree execution"
  fp: d50ad9efcbc1
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:47:34.195Z"
- id: CONF-002
  kind: confusion
  description: "correction to the prior cross-worktree gate observation: harness checks stayed in u-t; lean-ctx tee last is process-global and raced with u-r, so it displayed the sibling newest tee instead of the exact u-t harness_checks artifact. The actual u-t failure was only missing FS3_TEST_DATABASE_URL"
  severity: degrading
  workaround: "read the exact tee artifact named by the failed command, never tee last in a fleet"
  suggested_encoding: "scope tee last to cwd/session or print an ambiguity warning when the newest tee belongs to another worktree"
  fp: 3d6e9eae0c8e
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:48:54.656Z"
- id: DL-003
  kind: difficulty
  description: "merged u-t harness checks stopped after cargo test --all because the production migration guard read version=13 before, then could not parse ~/.config/flowspace3/config.toml after: unknown field agent; the after value was empty, so the gate reported a production change even though the second probe could not measure a version"
  severity: blocking
  workaround: "stopped without rerunning and reported the exact artifact to the PM"
  suggested_encoding: "guard the real config against concurrent test mutation and distinguish an unreadable after-probe from a measured schema-version change"
  fp: b3fe2e8d3ea1
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T04:04:32.528Z"
