# fs3 — brief / overview
**Written**: 2026-08-26 · **Source**: Jordan's direction (spine ruling 102700) + fs2 prior-art
survey · **Status**: pre-plan input — the `1b plan` pass consumes this; decisions here are
direction, not yet a spec.

## What fs3 is

**flowspace3 (fs3)**: semantic code search, rebuilt from scratch. Parse a codebase into
individual code elements (functions, classes, methods, markdown sections), enrich each with
an LLM summary and vector embeddings, and query by meaning, text, or regex — from the CLI
and (eventually) as an MCP server for AI agents. Same idea as fs2
(`/Users/jordanknight/substrate/fs2/flow_squared`), radically simpler machinery.

## Design pillars (Jordan, 2026-08-26)

1. **Rust.** tree-sitter used **directly** for AST parsing (first-class Rust bindings;
   rayon-parallel parse).
2. **No SCIP / Sourcegraph toolchain.** Cross-file relationship resolution is dropped. If
   ever needed: tree-sitter import queries + name heuristics as plain rows — not v1.
3. **No graph store, no pickle.** All data lives in **one central Postgres + pgvector** —
   every repo's index in one place, queryable with SQL, no whole-graph load into memory,
   no per-repo state files.
4. **Git-native incrementality (the worktree fix).** fs2 forced copy-graph-then-rescan per
   worktree. fs3 keys derived data by **git blob SHA**: indexing any worktree/branch/commit
   is `git ls-tree -r` → diff blob set against the store → only new blobs pay
   parse/summarize/embed. "A git tree of the files, with the flowspace version of each file
   in pg-vector."
5. **fs2's enrichment shape survives**: split file → elements; summarize the file AND each
   element (method / class / md section); embed both raw content and summary. Online or
   local LLM/embedders. Parallel by default.

## Architecture sketch (current thinking — plan pass to firm up)

**Two-layer store, mirroring git's object model:**

- **Content layer (immutable, append-only)** — keyed by blob SHA (+ model/prompt version
  for derived rows): parsed elements (node path, kind, span, source), summaries,
  embeddings (pgvector columns, HNSW-indexed). Pure functions of content: never
  invalidated, only superseded; deduped across branches, worktrees, and repos.
- **Ref layer (cheap, mutable)** — repo / branch / worktree / commit → the set of
  (path, blob SHA) it contains. Updating a ref after a commit is a tree diff, milliseconds.
  Dirty working files: `git hash-object` the content — same mechanism, no special case.

**Pipeline:** scan (git tree walk, gitignore-free — git already knows) → parse
(tree-sitter, per-language node → universal element kinds à la fs2's classify) → summarize
(LLM, async pool, rate-limited, batchable) → embed (online or local; batch APIs) → store
(PG upserts). Every stage skips blobs the store already has at the current model version.

**Search:** semantic (pgvector HNSW over code + summary embeddings), text (tsvector),
regex (over stored source) — hybrid ranking in SQL; folder-facet counts via GROUP BY
(fs2's envelope shape worth keeping).

## What fs2 taught us (keep / kill)

| Keep | Kill |
|---|---|
| Universal element classification (callable/type/section/…) over tree-sitter kinds | NetworkX graph + pickle store (and the MCP preload plan it required) |
| Dual embedding (raw content + AI summary) — semantic hits on either | SCIP adapters ×4 languages |
| Node-id style addressing (`callable:path:Qualified.Name`) | ~40-file adapter/fake matrix; Clean-Architecture ceremony |
| JSON envelope with pagination + per-folder hit counts | Layered config machinery (keep: env + one yaml) |
| Online/local provider choice for LLM + embeddings | Copy-graph-per-worktree workflow |

## Open questions (for the plan / workshops)

1. PG deployment story: one shared central instance vs per-machine; docker-compose vs
   existing substrate PG; the "new machine in 30s" answer.
2. Surface for v1: CLI only, or CLI + MCP from the start? (fs2's MCP is its highest-value
   consumer surface.)
3. Local embedder/LLM choice in Rust (ort/candle; which models).
4. Schema versioning: keying by (blob, model, prompt-version) and pointer-flip on
   re-embed — confirm; GC policy for unreferenced content.
5. Summarization cost control: which element kinds get summaries (fs2 filtered these) and
   batching strategy.
