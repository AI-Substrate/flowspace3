- id: CONF-001
  kind: confusion
  description: "Packet says compose Postgres is already up on host port 5433, but harness boot --json reported compose degraded: service db is not running."
  severity: degrading
  workaround: "Did not modify compose; reported to PM as instructed."
  suggested_encoding: "Make boot check the plan's configured external FS3_TEST_DATABASE_URL before declaring the local compose db absent."
  fp: d79ad98b4254
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:15:43.772Z"
- id: DL-001
  kind: difficulty
  description: "Rust LSP references returned no references for exported search_elements although crates/daemon/src/search.rs:296 and :303 call it; this makes the required symbol-level callsite check unreliable."
  severity: degrading
  workaround: "Used the already-read direct callsite as evidence and will retry after LSP reload."
  suggested_encoding: "Add a harness doctor check that waits for rust-analyzer workspace indexing and verifies a known cross-crate reference query."
  fp: 5aa0377b7f89
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:18:16.380Z"
- id: DL-002
  kind: difficulty
  description: "A per-worktree CARGO_TARGET_DIR build still blocked three times on Cargo's shared package-cache lock before compiling the isolated fixture; total no-run compile was 38s."
  severity: degrading
  workaround: "Waited for Cargo's global package-cache lock; target artifacts remained isolated."
  suggested_encoding: "Have the harness serialize or prefetch shared Cargo package-cache access before parallel per-seat builds."
  fp: 52e6f09c6ed2
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:37:58.505Z"
