- id: CONF-001
  kind: confusion
  description: "pij inbox --wait rejects rs-resident pushed seats with E-RS even though the generic pij skill says external peers should block there; packet-specific waiting method was not surfaced before the attempted wait"
  severity: degrading
  workaround: "Rely on the rs pushed turn and do not poll or block pij inbox"
  suggested_encoding: "Teach the pij skill's delivery-owned waiting rule to distinguish rs pushed seats explicitly and prescribe yielding after the ack"
  fp: ba08a19974e5
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T04:40:58.743Z"
- id: DL-001
  kind: difficulty
  description: "The mandatory harness checks gate refuses before lint/tests unless FS3_TEST_DATABASE_URL is set, but packet 015 explicitly limits validation to parsers/core and forbids database use; there is no database-free harness gate mode"
  severity: blocking
  workaround: "Ran cargo test -p fs3-core -p fs3-parsers successfully and stopped before introducing a database contrary to packet scope"
  suggested_encoding: "Add a harness checks package-scope mode or a parser-core gate that retains fmt/clippy/arch checks without requiring database credentials"
  fp: 35181f9cf14f
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T05:08:57.433Z"
- id: DL-002
  kind: difficulty
  description: "DB-backed harness checks ran for 12 minutes then fs3-test-suite exited 124 after reporting passing suites; the gate output does not name which command exceeded its timeout or expose a timeout override in the verdict"
  severity: blocking
  workaround: "Preserved the exclusive slot and inspected the checks help before deciding whether a deterministic retry is possible"
  suggested_encoding: "Have fs3-test-suite print the active command and timeout budget on exit 124, plus a targeted rerun command"
  fp: 1fe820bf346e
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T05:21:53.152Z"
- id: DL-003
  kind: difficulty
  description: "harness commit connected to collector and created 3649c0fdbe96e1cd4d47b4b76fa6bca58a82d1b1, but verification reported refs/notes/ai missing with no buffer to replay"
  severity: degrading
  workaround: "Retain the harness commit receipt and continue; no attribution buffer exists to drain"
  suggested_encoding: "Make direct-verified note misses include a durable recovery command in the untruncated top-level output"
  fp: a22eda927e2f
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T05:23:47.296Z"
