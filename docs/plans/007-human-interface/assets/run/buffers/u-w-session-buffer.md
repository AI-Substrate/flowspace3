- id: CONF-001
  kind: confusion
  description: "lean-ctx ls on absolute /Users/jordanknight/.cargo/registry/src returned the current flowspace3 repository tree instead of the requested Cargo registry directory."
  severity: degrading
  workaround: "Use direct exact-path tooling only after resolving the registry hash; do not trust that directory map."
  suggested_encoding: "Make lean-ctx ls preserve and print the resolved absolute target, and fail when it maps outside or back to cwd unexpectedly."
  fp: c3ca43b5dc3d
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T03:33:38.900Z"
