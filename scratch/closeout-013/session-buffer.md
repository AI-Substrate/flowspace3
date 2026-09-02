- id: DL-001
  kind: difficulty
  description: "Serena initial-instructions MCP timed out twice before coding; the direct Rust LSP is configured, so continue via xd://lsp and report the MCP path as degrading friction."
  severity: degrading
  workaround: "Use the configured xd://lsp rust-analyzer surface."
  suggested_encoding: "Make Serena initial-instructions return a bounded actionable timeout envelope or remove it as a mandatory duplicate when native LSP is healthy."
  fp: 3abe25b65583
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:39:48.967Z"
- id: CONF-001
  kind: confusion
  description: "RS identity mismatch during canary: plain pij whoami returned pij-imperial-weasel, but pij whoami --json refuses E-RS and pij node show says E-NOID; spawn metadata had to come from PIJ_SPAWN_ID."
  severity: degrading
  workaround: "Use plain pij whoami plus PIJ_SPAWN_ID and the runtime-provided model; do not pij adopt."
  suggested_encoding: "Give RS whoami a schema-safe JSON envelope and make its returned identity queryable by node/session inspection."
  fp: 3fbed9e54915
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:41:28.355Z"
- id: DL-002
  kind: difficulty
  description: "Worktree harness docker up cannot start the isolated stack: compose hard-codes /flowspace3-db, which conflicts with the already-running main/prod container; boot therefore reports db stopped although a same-named container exists."
  severity: degrading
  workaround: "Use FreshDatabase against the required :5433 endpoint with a seat-specific database label; never target the prod daemon on :7373."
  suggested_encoding: "Make worktree compose container names project-scoped and make boot distinguish name collision from a stopped service."
  fp: 72ad398d1d18
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:43:11.748Z"
- id: DL-003
  kind: difficulty
  description: "Approved implementation is blocked before task 0101: node_modules/.bin/ddocs is absent, matching harness doctor, but builder discipline and the prime ruling require ddocs for task progress and the ac-0005 amendment. Manual edits are forbidden."
  severity: blocking
  workaround: "Stop and ask o-prime for the canonical installed command or permission to restore the package; do not run npx or hand-edit deterministic documents."
  suggested_encoding: "Boot should fail fast when an active plan requires deterministic-document mutation but the repo-local ddocs CLI is absent."
  fp: 75785867247a
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:43:21.921Z"
- id: DL-004
  kind: difficulty
  description: "rust-analyzer LSP references for exported search_elements returned no references despite verified imports/calls in crates/store/tests and daemon code. Symbol-level coverage cannot be trusted for this packet."
  severity: degrading
  workaround: "Use exact-identifier repository search for callers and verify every edited span by exact read; do not use LSP rename."
  suggested_encoding: "Add a harness LSP canary that queries a known exported symbol and fails when rust-analyzer returns zero despite textual callsites."
  fp: 1a2b983e9239
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T01:45:11.808Z"
- id: DL-005
  kind: difficulty
  description: "Focused search_plan_shape test ran 648 seconds then lost the Postgres connection with UnexpectedEof during EXPLAIN. The prod-shaped old-query mutation is too expensive/unsafe for a routine test and leaked its scratch database on panic."
  severity: blocking
  workaround: "Do not rerun. Inspect the failing EXPLAIN boundary, clean only the named scratch database, and redesign mutation proof to avoid ANALYZE on the pathological old plan while retaining new-query ANALYZE."
  suggested_encoding: "Add a bounded query timeout to plan-shape tests and make scratch database cleanup panic-safe with the shared fs3_testkit helper."
  fp: 5128c3f475c1
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:06:18.325Z"
- id: DL-006
  kind: difficulty
  description: "harness commit created WIP commit 27d214fe45f3b1e89b0078cef68672d23f336ef7 in direct-verified mode but reported verify=missing: the refs/notes/ai attribution note did not land."
  severity: degrading
  workaround: "Keep the named commit SHA and report the missing note to o-prime; do not rewrite or rollback the commit."
  suggested_encoding: "Make the collector health gate prevent direct-verified from sounding healthy when the post-commit note is missing, and provide an in-command repair receipt."
  fp: b36089175cbd
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:09:33.814Z"
- id: DL-007
  kind: difficulty
  description: "First focused test on the newly ruled :5434 test postmaster failed before DDL: maintenance connection to postgres timed out after 5 seconds. No container operation or retry was attempted."
  severity: blocking
  workaround: "Stop and ask o-prime to confirm readiness/credentials for flowspace3-db-test; never fall back to :5433."
  suggested_encoding: "Add the separate test-postmaster readiness probe to harness boot and make the test command wait boundedly with a clear service-not-ready verdict."
  fp: cb0abcfcbe64
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:11:26.177Z"
- id: DL-008
  kind: difficulty
  description: "Separate :5434 test postmaster answers flowspace3_test but rejects the standard maintenance connection to database postgres with SQLSTATE 28000 (no pg_hba.conf entry for host 192.168.97.1/user flowspace3/database postgres). FreshDatabase-style isolation cannot create per-run databases."
  severity: blocking
  workaround: "Stop and ask o-prime to allow the maintenance database or provide an approved admin URL; never use :5433 or weaken isolation locally."
  suggested_encoding: "Make test-postmaster readiness prove the exact maintenance URL and CREATE/DROP scratch-database contract used by FreshDatabase, not only select 1 on the base database."
  fp: 753cefaa3994
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:28:31.092Z"
- id: DL-009
  kind: difficulty
  description: "On the separate :5434 test postmaster, the rewritten shipped search query exceeded the binding 30-second statement_timeout on the 50k-element/10k-smart corpus. The red contract verdict occurred before the old non-ANALYZE mutation check."
  severity: blocking
  workaround: "Stop immediately; do not rerun or raise the timeout. Ask o-prime before using non-ANALYZE plan inspection to redesign the join while preserving HNSW ordering."
  suggested_encoding: "Keep the 30-second statement_timeout and separate test postmaster as permanent backpressure; add a cheap non-ANALYZE preflight that names loss of the vector index before ANALYZE runs."
  fp: c76d6a7b86d0
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:31:40.183Z"
- id: CONF-002
  kind: confusion
  description: "RS inbox truncated o-prime's inline ask-006 ruling after 'Assert in the…', then pij tail could not recover it because pij-binding-magpie is not in the registry (E-NOID). The binding test assertion is missing."
  severity: blocking
  workaround: "Ask o-prime to persist and resend the full ruling as a file pointer; do not infer the truncated contract."
  suggested_encoding: "RS inbox should spill long messages to a durable file automatically or emit a recoverable message address; RS peers should be tail-addressable."
  fp: 4d2d395807b5
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:33:33.534Z"
- id: DL-010
  kind: difficulty
  description: "Focused daemon search regressions on separate :5434 found three scoped-starvation failures: expected 10/10, 10/10, and 1/1 semantic hits but the rewritten search returned zero. Existing expansion contract is red; no further tests ran."
  severity: blocking
  workaround: "Stop and report to o-prime. Do not raise expansion bounds or weaken tests; preserve the existing contract."
  suggested_encoding: "Keep search_scope_starvation in the focused admission gate so post-filter rewrites cannot silently return empty before candidate expansion."
  fp: e423391b6c9a
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T02:47:57.221Z"
- id: DL-011
  kind: difficulty
  description: "After merging main, pij inbox cannot receive o-prime's queued message: rs wire v1 is unsupported by the current v2 extension; CLI refuses fallback because delivery state is unknown."
  severity: blocking
  workaround: "Do not retry/fallback/adopt. Ask o-prime via send to persist the message as the next durable reply file."
  suggested_encoding: "Version-negotiate rs inbox responses or keep the client compatible across daemon rolling upgrades; auto-spill queued messages to the agent record when versions differ."
  fp: 96e6d51ac6f2
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T03:50:42.178Z"
- id: DL-012
  kind: difficulty
  description: "Final harness checks on head beee1491 changed the production database schema version from 22 to 23 despite FS3_TEST_DATABASE_URL pointing at the separate :5434 test postmaster. Gate emitted the binding STOP verdict; no rerun attempted."
  severity: blocking
  workaround: "Stop immediately, retain the exclusive slot, report exact before/after and head to o-prime, and perform read-only source diagnosis only if ruled."
  suggested_encoding: "Make the production migration guard prevent the write rather than detecting it after cargo test --all, and seal every spawned daemon/database URL before process start."
  fp: a43d08504c21
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T04:21:35.434Z"
