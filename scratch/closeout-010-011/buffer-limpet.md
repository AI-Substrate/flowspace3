- id: DL-001
  kind: difficulty
  description: "rs-resident coder could not send to legacy-resident o-prime: pij send failed E-RS twice and required file-polled acknowledgement"
  severity: blocking
  workaround: "Persisted ACK under .harness/temp/agent for o-prime polling"
  suggested_encoding: "Make pij send bridge rs and legacy registries or expose a deterministic cross-generation send command"
  fp: e47f603f2b8b
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:09:25.906Z"
- id: DL-002
  kind: difficulty
  description: "builder implement instructions require node_modules/.bin/dd, but this worktree has no such binary; mandated task-state update command exits 127"
  severity: degrading
  workaround: "Resolve the repository-owned ddocs update surface before changing task state"
  suggested_encoding: "Have builder instructions point to the installed deterministic-document CLI or make harness expose the state-update verb"
  fp: eab2d2da2e09
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:13:57.438Z"
- id: CONF-001
  kind: confusion
  description: "harness boot reported worktree compose db down, but docker compose up -d db could not start because the shared healthy flowspace3-db container already owns the fixed name; tests work via the existing port 5433 with an isolated flowspace3_test database"
  severity: degrading
  workaround: "Set FS3_TEST_DATABASE_URL to the isolated test database on the existing healthy shared container"
  suggested_encoding: "Teach harness boot to detect the healthy shared flowspace3-db/port before reporting the worktree compose service down"
  fp: ea3bb89df346
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:32:32.889Z"
- id: DL-003
  kind: difficulty
  description: "harness checks ran for several minutes as one opaque background job with no stage or progress output available, so the active failing or slow gate could not be identified while waiting"
  severity: degrading
  workaround: "Waited for the mandated aggregate verdict after targeted tests were already green"
  suggested_encoding: "Stream named fmt/clippy/test stage progress and expose the currently running check in the background-job envelope"
  fp: 007e56824182
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:38:06.408Z"
- id: DL-004
  kind: difficulty
  description: "prod has five failed embed rows but only four dedupe keys: jobs 1316706 and 1323215 share key embed:git:github.com/AI-Substrate/pij:raw:043365...; requeue_failed updates every eligible row in one statement, so the pending/running unique dedupe index may abort the entire boot sweep"
  severity: blocking
  workaround: "Stopped before bounce and asked o-prime to rule a duplicate-safe requeue or store fix"
  suggested_encoding: "Make requeue_failed select at most one failed row per dedupe key and terminally retire superseded duplicates, with a regression fixture"
  fp: 30fc3cba04ed
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:41:00.544Z"
- id: DL-005
  kind: difficulty
  description: "after prod drain job 1344012 was done, flowspace3 search --source conversation for exact payload phrase Stores overlap so values are never summed or averaged returned zero results"
  severity: blocking
  workaround: "Retrying shorter phrases and inspecting the recovered conversation rows before declaring AC-0006"
  suggested_encoding: "Add a drain verification command that resolves a completed embed job payload hash to its searchable conversation address and reports index visibility"
  fp: b036396a30d3
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T23:57:25.688Z"
- id: CONF-002
  kind: confusion
  description: "AC-0006 premise is false: failed job 1344012 uses conv:recovery only as a namespace for default-provider missing-vector batches; its six payload hashes resolve to document sections plus the empty hash, not a non-empty conversation turn, so no recovered-turn search address exists"
  severity: blocking
  workaround: "Resolved every payload hash against elements and read enrich.rs RECOVERY_IDENTITY rationale before refusing a semantic false-positive as proof"
  suggested_encoding: "Expose recovery-job item provenance or make acceptance templates distinguish conv:recovery namespace from stored conversation content"
  fp: 1f8b4fbeaa75
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:00:39.429Z"
- id: DL-006
  kind: difficulty
  description: "after o-prime reported prod bounced on 7fdf6fc and drain succeeded, the ruled AC-0006 document search failed FS3-E-DAEMON-UNAVAILABLE at 127.0.0.1:7373"
  severity: blocking
  workaround: "Running flowspace3 doctor and notifying o-prime instead of starting or bouncing prod from the coder seat"
  suggested_encoding: "Make the bounce command verify sustained daemon readiness through the acceptance read-back window"
  fp: bdb6a7edd7ed
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:01:46.180Z"
