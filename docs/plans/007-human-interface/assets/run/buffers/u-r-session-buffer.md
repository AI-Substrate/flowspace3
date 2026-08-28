- id: DL-001
  kind: difficulty
  description: "rust-analyzer reload and diagnostics succeeded, but LSP references for fs3_cli::render::render returned no references despite crates/cli/src/main.rs calling render::render at emit; symbol lookup is incomplete in this worktree"
  severity: degrading
  workaround: "Read the known frozen callsite directly and use targeted diagnostics after edits"
  suggested_encoding: "Add an LSP health probe that asserts a known cross-target reference after reload"
  fp: a1617fc4b9e4
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:33:40.422Z"
- id: DL-002
  kind: difficulty
  description: "Frozen get.json cannot deserialize into fs3_core::views::read::GetPayload: parent Outline rows omit children, but Outline.children has skip_serializing_if without serde(default); --human therefore falls back to JSON for a covered verb"
  severity: blocking
  workaround: "Stopped before changing the frozen core seam; requested PM ruling on Value-based rendering versus phase-1 core fix"
  suggested_encoding: "Round-trip every frozen response through its authoritative fs3_core::views DTO in the contract suite"
  fp: dad1c746abda
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:38:43.520Z"
- id: DL-003
  kind: difficulty
  description: "After merging daemon authentication, u-r add-progress GET /events has no way to authorize: DaemonClient keeps key_path private and exposes no event request; progress currently uses raw reqwest and would silently receive 401 on the live daemon"
  severity: blocking
  workaround: "Stopped before widening the client.rs fence; requested PM interface ruling"
  suggested_encoding: "Expose one authenticated streaming request method from DaemonClient or freeze an authorization accessor for non-envelope streams"
  fp: 8d85471a1658
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:53:46.993Z"
- id: DL-004
  kind: difficulty
  description: "harness checks migration guard reported production version 13 -> blank because its AFTER probe could not parse ~/.config/flowspace3/config.toml: unknown field agent; it labels a probe failure as a production migration incident"
  severity: blocking
  workaround: "Stopped and did not rerun; reported exact before/after probe failure to PM"
  suggested_encoding: "Migration guard must distinguish an unreadable after-probe from a changed schema version and report config-version skew without alleging a write"
  fp: eb405a1b4ffd
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T04:06:10.229Z"
