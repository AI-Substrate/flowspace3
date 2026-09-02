- id: DL-001
  kind: difficulty
  description: "Reviewer seat could not dogfood flowspace3 search: the flowspace MCP server reports 'MCP server not connected', and the only other search surface is the prod daemon on :7373, which the review fence forbids touching. A read-only reviewer therefore has NO usable semantic-search surface and must fall back to grep, which AGENTS.md explicitly discourages."
  severity: degrading
  workaround: "Fell back to grep + lsp for consumer discovery; enumerated BoundListener/publish call sites by regex instead of by meaning."
  suggested_encoding: "Give review worktrees a read-only search path that does not require the prod daemon: either a scratch-daemon recipe in the reviewer packet (config dir + :5434 + ephemeral port, index the worktree, search, drop), or make the flowspace MCP server auto-reconnect and point at a per-worktree index."
  fp: 9c824b91b351
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T09:11:54.331Z"
