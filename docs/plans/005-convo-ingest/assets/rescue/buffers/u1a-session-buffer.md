- id: DL-001
  kind: difficulty
  description: "Editor tool resolved a relative path against the session cwd (main clone) instead of the worktree, so a module registration landed in another seat's tree; the subsequent green build compiled a tree that had never seen the 25KB file. Two byte-identical files in two trees also hash to the SAME content-derived edit tag, so re-reading for a 'fresh' tag returns an identical one and the tag offers zero protection."
  severity: blocking
  workaround: "Absolute paths for every read and edit; proved registration by the test binary importing the module rather than by an exit code."
  suggested_encoding: "Guidance must be 'absolute paths always', never 'absolute paths when the tag looks wrong' — for identical files the tag never looks wrong. Ideally the edit tool should refuse or warn on a relative path when the session cwd is not the git worktree root."
  fp: 1040954096fc
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T01:51:42.258Z"
