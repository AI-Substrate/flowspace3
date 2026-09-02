- id: DL-001
  kind: difficulty
  description: "rust-analyzer references on auth.rs StagedAuth::publish returned no references despite visible same-file callsites at auth.rs:164 and :194"
  severity: degrading
  workaround: "Verify callsites with exact source reads and exact-identifier search"
  suggested_encoding: "Add an LSP health probe that checks a known same-file reference before coder packets rely on rust-analyzer"
  fp: cf567027f650
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T07:12:13.935Z"
- id: DL-002
  kind: difficulty
  description: "the mandated :5434 base database is at migration 0024 while this main-based worktree binary carries 0023, so a real-daemon probe against flowspace3_test exits before bind"
  severity: degrading
  workaround: "Create a per-run FreshDatabase on the :5434 postmaster and point both sealed daemons at it"
  suggested_encoding: "Make subprocess integration helpers allocate a per-run child database rather than use the shared base database as application state"
  fp: ef4f2b37b0af
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T07:29:13.636Z"
