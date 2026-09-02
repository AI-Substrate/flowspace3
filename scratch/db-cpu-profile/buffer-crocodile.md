- id: DL-001
  kind: difficulty
  description: "rust-analyzer references returned no callers for exported FreshDatabase methods; Serena cross-check timed out for create and sweep while finding cleanup callers"
  severity: degrading
  workaround: "Keep public signatures unchanged; use Flowspace and exact identifier search only where caller inventory becomes necessary"
  suggested_encoding: "Add a harness diagnostic that verifies rust-analyzer workspace indexing before reporting an empty reference set"
  fp: 60fc892d0d4b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:46:40.640Z"
- id: DL-002
  kind: difficulty
  description: "orphan-sweep integration test using SystemTime::now selected pre-existing shared-server orphans in addition to its scratch names; an exact equality assertion failed after creating three scratch databases"
  severity: degrading
  workaround: "Use a synthetic near-epoch clock so only names minted at epoch 1 are old; remove failed-run scratch databases explicitly"
  suggested_encoding: "Provide a testkit helper that scopes destructive orphan-sweep fixtures without depending on ambient shared-server contents"
  fp: 0cf2e0c3a5df
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:53:24.920Z"
- id: DL-003
  kind: difficulty
  description: "harness commit 05d7d87cdfee reported connected ingress but no refs/notes/ai entry appeared within 5 seconds; commit stands with authorship unrecorded"
  severity: degrading
  workaround: "Keep the named commit outcome in the coder report; do not claim attribution"
  suggested_encoding: "Make harness commit's missing-note remediation available without truncation and add a deterministic retry/check command"
  fp: ada2eb1bfd2b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:12:55.023Z"
- id: DL-004
  kind: difficulty
  description: "prod-safe test-server read-only count failed at 2026-09-02 during review delta: flowspace3-db returned FATAL database system is shutting down before list_orphans example could run"
  severity: blocking
  workaround: "Stopped immediately; no retry, restart, compose action, sweep, or further database command"
  suggested_encoding: "Expose shared-postmaster lifecycle/recovery as a harness sensor that attributes the active seat and blocks mutation proofs before connection attempts"
  fp: 8dbe70a85050
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:39:08.708Z"
- id: DL-005
  kind: difficulty
  description: "review-delta hard checkpoint target failed: cargo test -p fs3-store passed 137 tests, but window 2026-09-02T01:48:52Z..01:53:07Z contained 83 immediate forced-checkpoint starts and pg_stat_bgwriter checkpoints_req rose 1171 to 1299 (+128), versus reviewer baseline target 25"
  severity: blocking
  workaround: "Stopped without retry or tuning; preserved window, test artifact, log count, and counter delta for o-prime ruling"
  suggested_encoding: "Add a harness checkpoint-window sensor that isolates the calling test processes and reports concurrent external DDL attribution before judging the threshold"
  fp: 7fbe3cc75eb0
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:53:33.264Z"
- id: DL-006
  kind: difficulty
  description: "three parallel ddocs set operations against plan.dd.json raced on the shared .tmp rename; ac-0002 failed E452 while ac-0001/ac-0003 reported success"
  severity: degrading
  workaround: "Never parallelize ddocs writes to the same document; rerun the failed address sequentially and rebuild"
  suggested_encoding: "ddocs set should use unique atomic-write temp files or reject concurrent writers with a clear lock error"
  fp: a239920f08ee
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:56:58.840Z"
- id: DL-007
  kind: difficulty
  description: "review-delta harness commit f3aec311b569 returned degraded after connected ingress, repeating the missing refs/notes/ai attribution seen on 05d7d87"
  severity: degrading
  workaround: "Name the degraded attribution outcome; continue using the committed SHA without claiming note delivery"
  suggested_encoding: "Make harness commit retry or deterministically verify note delivery after connected-ingress misses"
  fp: 81831a08e9e8
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:00:02.653Z"
- id: DL-008
  kind: difficulty
  description: "exclusive harness checks failed while compiling fs3-store because the worktree target filesystem returned ENOSPC; migration guard then could not read a schema version"
  severity: blocking
  workaround: "Keep gate slot, remove this worktree's disposable cargo artifacts with cargo clean, rerun harness checks once"
  suggested_encoding: "Add pre-gate disk-space/target-size backpressure with an actionable cargo-clean recommendation before compilation starts"
  fp: 3afd355211ec
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:09:36.631Z"
