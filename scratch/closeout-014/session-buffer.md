- id: DL-001
  kind: difficulty
  description: "harness doctor is degraded before jobs-retention work: transient scratch conventions unprotected, git-ai collection absent, two generated dd siblings missing, and standalone dd CLI unavailable"
  severity: degrading
  workaround: "Proceeding only with canary investigation; no code before o-prime ruling"
  suggested_encoding: "Make harness boot distinguish packet-local blockers from known repository-wide degraded layers"
  fp: e14cbb23f825
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:40:26.338Z"
- id: CONF-001
  kind: confusion
  description: "rust-analyzer LSP references returned no queue_depth callers even though the profile and indexed code identify runner.rs and status.rs callsites"
  severity: degrading
  workaround: "Cross-checking with Serena symbol references and exact identifier search before ack"
  suggested_encoding: "Add an LSP smoke probe that asserts cross-crate references resolve for a known exported store symbol"
  fp: 7a5202c06a28
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:42:21.667Z"
- id: DL-002
  kind: difficulty
  description: "pre-work harness boot is degraded because Docker Compose service db is not running; build passed but integration proof cannot run yet"
  severity: blocking
  workaround: "Stopping before code as required; will request test DB readiness/gate slot from o-prime in the canary ack"
  suggested_encoding: "Have harness boot name the isolated :5433 test-postmaster command when production compose db is intentionally not the test target"
  fp: aef3805b9beb
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:42:57.617Z"
- id: DL-003
  kind: difficulty
  description: "exact grep for worktree_reconcile_ticks across the whole repository timed out at 30s during retention config-doc cleanup"
  severity: degrading
  workaround: "Narrowing searches to known config and documentation paths"
  suggested_encoding: "Expose a fast config-key reference command or make repository exact-text search exclude large generated probe output by default"
  fp: 3f96a7918ee0
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:01:31.878Z"
- id: DL-004
  kind: difficulty
  description: "flowspace3 semantic search for the newly written retention path failed with FS3-E-STORE-QUERY-FAILED: pool timed out waiting for an open connection"
  severity: degrading
  workaround: "Following the envelope's re-run-once instruction; source correctness remains covered by focused tests"
  suggested_encoding: "Have search report pool occupancy and the blocking query or distinguish saturation from a stopped store"
  fp: 254c80a9a736
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:06:49.017Z"
- id: CONF-002
  kind: confusion
  description: "rust-analyzer diagnostics continue to type StatusReport.retention as a non-Option after the worktree file was changed to Option and LSP reload completed; exact file reads disagree with LSP"
  severity: degrading
  workaround: "Treating exact worktree bytes as authority and deferring compiler confirmation until o-prime clears the database freeze"
  suggested_encoding: "Make LSP worktree roots explicit and expose which Cargo workspace/file snapshot supplied a diagnostic"
  fp: 39a4f5af684d
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:09:13.098Z"
- id: DL-005
  kind: difficulty
  description: "first focused store run against the newly mandated 127.0.0.1:5434/flowspace3_test timed out acquiring a connection in all three tests"
  severity: blocking
  workaround: "Stopped database-backed tests and did not touch flowspace3-db-test; asking o-prime to restore the separate postmaster"
  suggested_encoding: "Add a preflight that proves the dedicated test postmaster is accepting authenticated SQL before granting the test slot"
  fp: ba10923b7ff5
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:10:44.914Z"
- id: DL-006
  kind: difficulty
  description: "the cleared :5434 test postmaster accepts TCP but rejects the binding host URL: no pg_hba.conf entry for host 192.168.97.1, user flowspace3, database flowspace3_test, no encryption"
  severity: blocking
  workaround: "Stopped DB-backed tests without touching the container; o-prime must make the exact mandated host URL authenticate"
  suggested_encoding: "Make the dedicated-test health receipt execute select 1 through the exact host URL workers receive, not from inside the container"
  fp: ab37eafce957
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:28:54.785Z"
- id: DL-007
  kind: difficulty
  description: "focused daemon test spent about 153 seconds blocked on Cargo's shared package-cache lock despite a per-seat CARGO_TARGET_DIR; test itself finished in 1.68 seconds"
  severity: degrading
  workaround: "Kept one local cargo invocation at a time and waited; o-prime still serializes only full gates"
  suggested_encoding: "Give seats an isolated CARGO_HOME or a harness-level package-cache slot in addition to isolated target directories"
  fp: 37eb6a5b0a92
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:37:59.130Z"
- id: DL-008
  kind: difficulty
  description: "Serena code-intelligence initialized an untracked .serena directory in the packet worktree; it is outside the jobs-retention fence and must not enter the commit"
  severity: degrading
  workaround: "Leaving the directory untouched and excluding it from harness commit paths"
  suggested_encoding: "Configure Serena project metadata outside task worktrees or root-ignore .serena deterministically"
  fp: 151352c92f37
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:42:21.982Z"
- id: DL-009
  kind: difficulty
  description: "harness commit c5242ea reported connected ingress but refs/notes/ai verification was missing after the commit"
  severity: degrading
  workaround: "Commit outcome is explicitly named; proceeding with required push and reporting degraded attribution to o-prime"
  suggested_encoding: "Make harness commit retry note verification or emit the exact telemetry-nudge command when connected ingress misses"
  fp: 436163b2d6a3
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T04:06:17.344Z"
