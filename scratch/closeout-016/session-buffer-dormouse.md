- id: DL-001
  kind: difficulty
  description: "harness doctor reports extensions degraded because transient scratch is unprotected in this worktree"
  severity: degrading
  workaround: "Proceed only after boot verdict; avoid using transient scratch as proof"
  suggested_encoding: "Have worktree setup restore or verify harness convention protection before coder spawn"
  fp: ebeb13977ee5
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:31:39.402Z"
- id: DL-002
  kind: difficulty
  description: "harness boot degraded because the compose db service is not running in this coder worktree, while this packet requires the dedicated flowspace3-db-test postmaster on port 5434"
  severity: degrading
  workaround: "Use only the dedicated test postmaster and report the boot mismatch to o-prime before coding"
  suggested_encoding: "Teach harness boot to detect packet-specific dedicated test database mode or state which compose service is optional"
  fp: dc6dcb607ba9
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:33:04.724Z"
- id: CONF-001
  kind: confusion
  description: "flowspace3 status reports FS3-E-SCAN-UNPARSEABLE for docs/plans/014-jobs-retention/assets/reviews/cross-model-review.dd.json even though this worktree queue is empty"
  severity: degrading
  workaround: "Treat current plan search as usable but report the stale unrelated parse failure to o-prime"
  suggested_encoding: "Make status distinguish historical unrelated scan failures from current-root health or clear resolved last_error deterministically"
  fp: e8a82fa03eda
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:33:09.602Z"
- id: CONF-002
  kind: confusion
  description: "flowspace3 semantic search returned relevant watcher and plan hits but flooded each ddoc result with repeated address-target-untracked warnings for in-plan relative targets"
  severity: degrading
  workaround: "Use the relevant code hits and cite the warning noise to o-prime; do not broaden search"
  suggested_encoding: "Deduplicate ddoc findings per result or suppress known-valid relative targets from the search envelope"
  fp: 3c954339fe83
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:33:18.463Z"
- id: CONF-003
  kind: confusion
  description: "packet instructs bare pij report now, but rs rejects it with usage requiring positional did and next text and does not fall back"
  severity: degrading
  workaround: "Use durable ack and direct pij send; ask o-prime for the intended status-card arguments"
  suggested_encoding: "Update packet template to emit the required pij report now arguments or restore a valid bare status form"
  fp: 491b2de2e2ce
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:34:04.273Z"
- id: CONF-004
  kind: confusion
  description: "packet fences crates/store/src/worktrees.rs, but that file does not exist; semantic search locates worktree registration in crates/store/src/refs.rs outside the declared fence"
  severity: blocking
  workaround: "Stop before editing and ask o-prime to amend ownership/read scope"
  suggested_encoding: "Generate packet fences from symbol-resolved implementation paths and validate every fenced path exists before spawn"
  fp: b5f83b6097d3
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:34:57.831Z"
- id: DL-003
  kind: difficulty
  description: "the required test-database select 1 probe failed because psql is installed outside PATH or absent on this workstation"
  severity: degrading
  workaround: "Resolve Homebrew libpq prefix and invoke its psql binary directly against the exact URL"
  suggested_encoding: "Have harness boot or packet generation emit the repository-supported database probe command that works on an unlinked Homebrew libpq install"
  fp: 05c9afe7beaf
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:35:36.009Z"
- id: CONF-005
  kind: confusion
  description: "builder implement discipline requires a task in-progress state, but global ddocs schema-refused in-progress because task states are only unchecked, checked, blocked, human-skipped, or na"
  severity: degrading
  workaround: "Leave t1 unchecked while active and use task-boundary report files; set done_when and task to checked only after proof"
  suggested_encoding: "Align builder implementation guidance with the deterministic-document state vocabulary or add an explicit active-task field"
  fp: 814645363195
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:36:24.366Z"
- id: CONF-006
  kind: confusion
  description: "approved interface requires RootReport and status Root include_hidden fields, but their owners crates/core/src/views/roots.rs and status.rs are absent from the coder fence and public-envelope changes require o-prime ruling"
  severity: blocking
  workaround: "Stop before source edits and request explicit fence amendment alongside missing store refs.rs"
  suggested_encoding: "Have packet generation resolve interface symbols to owning files and include those files in the write fence before dispatch"
  fp: e562471cef6c
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:36:40.842Z"
- id: DL-004
  kind: difficulty
  description: "flowspace3 semantic search for t3 skip-ledger design failed with FS3-E-DAEMON-UNAVAILABLE at read-only prod :7373 after earlier doctor reported it healthy"
  severity: degrading
  workaround: "Do not start or touch prod; continue with exact source reads inside the ruled fence and report the outage to o-prime"
  suggested_encoding: "Keep the dogfood daemon supervised or let search name a safe read-only fallback index endpoint"
  fp: 897c8262f4d2
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:48:45.675Z"
- id: DL-005
  kind: difficulty
  description: "T4 scoped regression gate failed in daemon health test because the real daemon never served its free-port health endpoint or honoured FS3_CONFIG_DIR; packet says a red tripwire must stop and report"
  severity: blocking
  workaround: "Stopped without rerunning or changing the unrelated health path; requested o-prime ruling and exclusive-slot guidance"
  suggested_encoding: "Make the health integration test emit the spawned daemon stderr and exact config path when readiness times out, and serialize real-binary daemon tests if port/process contention is possible"
  fp: d25ec65829d4
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T06:59:07.275Z"
- id: DL-006
  kind: difficulty
  description: "the ruled isolated real-binary health test failed: spawned daemon never served its free-port health endpoint under FS3_CONFIG_DIR, despite the immediately preceding full harness checks completing green"
  severity: blocking
  workaround: "Wrote exact output to .harness/temp/agent/health-isolated.log, stopped without rerun or health-path changes, and released the gate slot"
  suggested_encoding: "Capture spawned daemon stderr and process exit in the test failure; make real-daemon readiness deterministic under host load"
  fp: 2681934b281e
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T07:08:11.434Z"
- id: CONF-007
  kind: confusion
  description: "T5 current pij corpus has only 379 tracked .pi/**/*.ts files and 378 indexed after opt-in, so the plan's inherited >=500 TypeScript threshold is impossible on today's checkout"
  severity: blocking
  workaround: "Captured exact default/opt-in counts and requested o-prime amendment to an all-current-eligible-files invariant while continuing the reachable named-function search proof"
  suggested_encoding: "Generate real-usage count thresholds from a pinned corpus commit or express them as all eligible tracked files with a recorded baseline"
  fp: 020348c2f468
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T07:14:17.029Z"
- id: DL-007
  kind: difficulty
  description: "T5 scratch daemon spent over five minutes draining unrelated historical jobs in the shared flowspace3_test database before reaching the newly mapped .pi extension scans; waiting on the named file timed out"
  severity: degrading
  workaround: "Use database state to measure the target root and identify exact pending target work while keeping the scratch daemon isolated from prod"
  suggested_encoding: "Give real-usage receipts a fresh database on the test postmaster or a root-scoped drain command so historical backlog cannot dominate proof latency"
  fp: 64f303f87af2
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T07:20:34.763Z"
