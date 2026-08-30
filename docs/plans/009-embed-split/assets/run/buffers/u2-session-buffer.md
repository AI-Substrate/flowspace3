- id: CONF-001
  kind: confusion
  description: "harness boot --json reported compose service db not running even though packet says flowspace3-db is already up on host port 5433; boot may be checking this worktree's compose project rather than host reachability"
  severity: degrading
  workaround: "Use PM-provided FS3_TEST_DATABASE_URL at 127.0.0.1:5433; do not mutate compose state"
  suggested_encoding: "Have boot distinguish reachable external test DB from inactive local compose project"
  fp: eeb65dd5f0c4
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:14:59.771Z"
- id: DL-001
  kind: difficulty
  description: "flowspace3 search timed out after 60s for embedding mint/prepare/batch architecture query, so semantic orientation returned no envelope"
  severity: degrading
  workaround: "Use packet line anchors and narrow source reads; retry a narrower semantic query later"
  suggested_encoding: "Bound search latency and return a typed timeout envelope with query diagnostics"
  fp: dcfc73c03193
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:16:13.999Z"
- id: DL-002
  kind: difficulty
  description: "rust-analyzer references returned no references for enrich::embed_items even though crates/daemon/src/runner.rs:642 calls it; diagnostics work, but reference discovery is incomplete"
  severity: degrading
  workaround: "Use source anchors for this packet and keep LSP diagnostics enabled"
  suggested_encoding: "Add an LSP health probe that resolves a known cross-module Rust reference"
  fp: 8c1f9b288aaf
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:17:56.797Z"
- id: DL-003
  kind: difficulty
  description: "harness checks cannot start after merge: global harness-engineering CLI imports missing node_modules/@ai-substrate/dd/index.js from dist/acts/flow.js"
  severity: blocking
  workaround: "Run the exact underlying cargo fmt/clippy/test gates while PM repairs global harness dependency"
  suggested_encoding: "Package or dependency-check @ai-substrate/dd before loading flow acts"
  fp: fc33c95ae60a
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T06:49:07.104Z"
- id: DL-004
  kind: difficulty
  description: "rust-analyzer still reports E0560 NewEmbedding.chunk_no after reloading, while cargo check/clippy/test compile the merged field successfully; LSP did not refresh dependency sources after git merge"
  severity: degrading
  workaround: "Trust compiler gates and cite stale LSP diagnostic separately"
  suggested_encoding: "Reload workspace dependency graph after git merges, not only the active file server"
  fp: 857c88330583
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T06:54:03.104Z"
