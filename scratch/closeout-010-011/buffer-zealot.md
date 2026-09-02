- id: DL-001
  kind: difficulty
  description: "Two flowspace3 semantic searches exceeded 60 seconds during conv resolution-path orientation; daemon doctor was healthy and the queue had no active work."
  severity: degrading
  workaround: "Use successful narrower searches plus exact grep/LSP for the required resolution-path census."
  suggested_encoding: "Have search enforce and report a bounded server-side timeout with phase diagnostics before the caller timeout."
  fp: cfa7f6bff891
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:11:39.756Z"
- id: CONF-001
  kind: confusion
  description: "Rust LSP references for conversations.rs::resolve_selector returned 'No references found', while exact grep found its direct call at daemon/src/ask.rs:294; a later LSP references query for read::get correctly returned nine references."
  severity: degrading
  workaround: "Cross-check the required conv resolution census with exact grep."
  suggested_encoding: "Add an LSP smoke probe comparing a known private Rust function reference against rust-analyzer output."
  fp: 55ecae78dc1a
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:13:25.041Z"
- id: DL-002
  kind: difficulty
  description: "Builder's mandated node_modules/.bin/dd task-state command is absent in this worktree (command not found) before tk-0101."
  severity: degrading
  workaround: "Locate the repo's actual deterministic-document CLI before mutating task state; do not hand-edit generated files."
  suggested_encoding: "Have harness boot validate and print the exact installed dd mutation command used by builder progress."
  fp: 8c05a7879677
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:18:52.954Z"
- id: CONF-002
  kind: confusion
  description: "harness boot reported compose service db stopped, but 127.0.0.1:5433 was already serving the isolated test database; starting this worktree's db failed because fixed container name flowspace3-db is owned by another checkout."
  severity: degrading
  workaround: "Use FS3_TEST_DATABASE_URL against the already-running shared Postgres and let each test create/drop unique databases."
  suggested_encoding: "Have boot detect a healthy compatible shared database before declaring compose stopped, or namespace compose container names per worktree."
  fp: 5b7aeb325396
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:21:29.138Z"
- id: DL-003
  kind: difficulty
  description: "Serialized harness checks stopped because the production database schema changed from version 22 to absent during cargo test --all. Per the gate verdict, no rerun or investigation was attempted."
  severity: blocking
  workaround: "Stopped immediately and routed the tripwire to o-prime in conv-verify-ask-002.md."
  suggested_encoding: "Identify and seal the test path that can reach production; keep this before/after schema tripwire as the mandatory guard."
  fp: 96384c79114d
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T22:49:01.159Z"
- id: CONF-003
  kind: confusion
  description: "Rust LSP rename reported one applied edit in convo_ingest.rs for QUERY_CONVERSATION_NOT_FOUND, but the callsite remained QUERY_CONVERSATION_NOT_INDEXED on disk; catalog edits did apply."
  severity: degrading
  workaround: "Verified with exact grep/read and corrected the single missed callsite manually after the symbol-aware rename."
  suggested_encoding: "Have the LSP edit device verify every reported workspace edit against disk and fail on partial application."
  fp: 58402b05b9be
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T23:31:24.399Z"
- id: DL-004
  kind: difficulty
  description: "The same Rust LSP rename inserted a stray bare QUERY_CONVERSATION_NOT_FOUND token after verify_seat's closing brace while leaving the intended callsite unchanged, causing a syntax error."
  severity: degrading
  workaround: "Removed the stray token and manually changed the intended callsite after inspecting the exact affected range."
  suggested_encoding: "Make LSP rename edits position-verified against the pre-edit document version and run a syntax check before reporting Applied."
  fp: fc19b64d7112
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-01T23:32:13.023Z"
