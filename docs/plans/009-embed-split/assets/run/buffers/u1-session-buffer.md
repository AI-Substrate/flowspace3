- id: DL-001
  kind: difficulty
  description: "harness boot --json exceeded 120 seconds before producing a verdict during packet orientation"
  severity: degrading
  workaround: "continue read-only orientation; rerun with a longer deadline before editing"
  suggested_encoding: "emit phase progress or a bounded timeout diagnosis during boot"
  fp: 0c7ed337ca64
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:14:36.974Z"
- id: CONF-001
  kind: confusion
  description: "rust-analyzer was configured but LSP references returned no references for exported NewEmbedding and put_embeddings despite visible store test callsites"
  severity: degrading
  workaround: "use exact-identifier grep after the required LSP query"
  suggested_encoding: "add an LSP health probe that rejects implausible empty reference results for known exported symbols"
  fp: d53370c71490
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:16:23.096Z"
- id: DL-002
  kind: difficulty
  description: "Focused fs3-store tests refused because configured FS3_TEST_DATABASE_URL database flowspace3_test_u1 does not exist, despite the packet describing it as a server selector"
  severity: blocking
  workaround: "stop and report to PM; do not start or reconfigure containers"
  suggested_encoding: "have dispatch provision each seat database selector or make the testkit connect through a guaranteed maintenance database"
  fp: 9b8cec8e1f93
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-30T05:34:27.264Z"
