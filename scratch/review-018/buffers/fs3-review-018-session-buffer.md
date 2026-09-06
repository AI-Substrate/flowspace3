- id: DL-001
  kind: difficulty
  description: "A green 'cargo test -p fs3-daemon' leaves a scratch Postgres database behind on :5434 (observed: fs3_worktreelife_* created at 19:45 by an all-green run). Reviewers and coders auditing their own fence have to attribute leftover databases by embedded timestamp to decide which are theirs, because a green run is not a clean run."
  severity: degrading
  workaround: "Listed pg_database before and after my work, converted the embedded epoch in each database name to a timestamp, and dropped only the ones inside my own window."
  suggested_encoding: "Either make FreshDatabase drop on unwind as well as on success, or add a 'harness db-scratch --list/--prune' that reports scratch databases with age and lets a seat prune only its own."
  fp: 7af4d7220052
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-06T09:51:16.746Z"
