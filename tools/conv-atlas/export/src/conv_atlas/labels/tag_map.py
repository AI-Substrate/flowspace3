"""Tag-normalisation map, as versioned data.

Summary tags are free-form (1-5 per summary, written by the summariser), so the
same idea arrives as `Rust`/`rust`, `tests`/`testing`, `git-status`/`git status`.
`normalize.normalize_tag` folds case, punctuation and plurals mechanically; this
module holds the judgement calls the mechanics cannot make.

Rules for editing (the map is applied AFTER the mechanical steps):
- Keys are written in mechanical-output form: lower-case, single spaces, `-`/`_`
  turned into spaces, last word singularised. A key in any other form never matches.
- Values must be fixed points: `normalize_tag(value) == value` (a test enforces it).
- Generic vocabulary only. The repo is public: no project, person or place names.
- Bump TAG_MAP_VERSION on any change; it is recorded in snapshot provenance.

Decisions:
- `test`, `tests`, `testing` -> `testing`; `<kind> testing` -> `<kind> test`
  (integration/regression/smoke/unit), so the plural and gerund forms meet.
- Status words and tool mechanics (`completed`, `bash`, `file write`, ...) are
  dropped: they describe how an agent worked, not what the conversation was about.
"""

from __future__ import annotations

TAG_MAP_VERSION = 3

# Words whose trailing "s" is not a plural. Checked on the last word only.
KEEP_S: frozenset[str] = frozenset(
    {
        "always",
        "alias",
        "analysis",
        "atlas",
        "canvas",
        "dns",
        "devops",
        "jenkins",
        "kubernetes",
        "lens",
        "macos",
        "news",
        "nodejs",
        "postgres",
        "redis",
        "series",
        "species",
        "windows",
    }
)

# alias -> canonical. Keys and values in mechanical-output form (see module doc).
SYNONYMS: dict[str, str] = {
    # testing
    "test": "testing",
    "integration testing": "integration test",
    "regression testing": "regression test",
    "smoke testing": "smoke test",
    "unit testing": "unit test",
    "e2e": "end to end test",
    "e2e test": "end to end test",
    "end to end testing": "end to end test",
    "type checking": "typecheck",
    "type check": "typecheck",
    "lint": "linting",
    # docs and process
    "doc": "documentation",
    "docs": "documentation",
    "handoff": "handover",
    "config": "configuration",
    "repo": "repository",
    "pr": "pull request",
    "prs": "pull request",
    "github pr": "pull request",
    # technology names whose spellings vary
    "postgres": "postgresql",
    "nodejs": "node.js",
    "node js": "node.js",
    "k8s": "kubernetes",
    "rust analyzer": "rust-analyzer",
    "sha 256": "sha-256",
    "sha256": "sha-256",
    "github action": "github actions",
    "ue5": "unreal engine",
    "unreal": "unreal engine",
    "js": "javascript",
    "apis": "api",
    "gh": "github",
}

# Tags that carry no topic: outcome/status words and tool mechanics.
DROP: frozenset[str] = frozenset(
    {
        # outcome / status words
        "acknowledgement",
        "acknowledgment",
        "approval",
        "completed",
        "completion",
        "completion status",
        "confirmation",
        "done",
        "failure",
        "general",
        "in progress",
        "misc",
        "miscellaneous",
        "no response",
        "n/a",
        "none",
        "ok",
        "other",
        "passing",
        "passing test",
        "progress",
        "result",
        "response",
        "status",
        "status update",
        "success",
        "summary",
        "task completion",
        "unknown",
        "update",
        # tool mechanics (how the agent worked, not what about)
        "background command",
        "background execution",
        "background job",
        "background process",
        "background task",
        "bash",
        "bash output",
        "command",
        "command execution",
        "command output",
        "directory listing",
        "file creation",
        "file inspection",
        "file listing",
        "file read",
        "file reading",
        "file write",
        "filesystem listing",
        "grep",
        "output file",
        "sed",
        "shell command",
        "task notification",
        "task output",
        "tool call",
        "tool output",
        "tool result",
        "truncated output",
    }
)

# Display forms for words inside labels (tags themselves stay lower-case).
DISPLAY: dict[str, str] = {
    "ai": "AI",
    "api": "API",
    "aws": "AWS",
    "ci": "CI",
    "cli": "CLI",
    "cpu": "CPU",
    "css": "CSS",
    "csv": "CSV",
    "dns": "DNS",
    "docker": "Docker",
    "duckdb": "DuckDB",
    "git": "git",
    "github": "GitHub",
    "gpu": "GPU",
    "graphql": "GraphQL",
    "html": "HTML",
    "http": "HTTP",
    "https": "HTTPS",
    "ios": "iOS",
    "javascript": "JavaScript",
    "json": "JSON",
    "jsonl": "JSONL",
    "llm": "LLM",
    "lsp": "LSP",
    "macos": "macOS",
    "mcp": "MCP",
    "node.js": "Node.js",
    "npm": "npm",
    "pr": "PR",
    "postgresql": "PostgreSQL",
    "python": "Python",
    "react": "React",
    "rust": "Rust",
    "sdk": "SDK",
    "sql": "SQL",
    "sqlite": "SQLite",
    "ssh": "SSH",
    "tui": "TUI",
    "typescript": "TypeScript",
    "ui": "UI",
    "url": "URL",
    "webgl": "WebGL",
}
