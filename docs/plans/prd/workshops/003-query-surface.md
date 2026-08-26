# Workshop 003 — Query surface
**Type**: CLI Flow + API Contract · **Date**: 2026-08-26 · **Author**: o-prime, direction agreed with Jordan (fs2 mined first — keep/drop verdicts baked in) · **Status**: AUTHORITATIVE
**Consumers**: the integration/daemon plan (query endpoints), CLI plan, MCP surface, conversations plan.

## Shape: one verb, three companions

```text
search  → ranked hits (lean rows + addresses)
get     → depth on ONE address (element w/ children, or conversation window)
tree    → structure browse (files/containers, or a conversation's turn outline)
status / doctor / add  → ops (out of scope here, exist already in PRD)
```

CLI and MCP expose the SAME service with the same parameters (fs2's one good parity property, kept). **JSON-only in v1** (Jordan ruling 2026-08-26): CLI and MCP both emit the envelope; human-readable rendering is a later additive layer. No `save_to_file` on tools (dropped — agents have file tools).

## Addresses — the universal currency

Every hit carries one; `get`/`tree` accept them; stable across re-parses.

```text
el:<repo>/<path>::<container>::<name>     # element (address column)
conv:<guid>                                # conversation
conv:<guid>#t<ord>                         # one turn
```

## Modes

| Mode | Matcher | Notes |
|---|---|---|
| `auto` (default) | semantic unless the pattern is STRONG regex evidence (anchors/classes — not a lone `?` or `(`; fs2's "how does auth work?"→regex trap explicitly killed) | fallback text when no embedder configured, error says why |
| `semantic` | query → embed → HNSW | default ranking = hybrid (below) |
| `text` | PG full-text (`tsvector`) over raw_text + smart text | replaces fs2's escape-to-regex |
| `regex` | PG `~` with statement timeout | for surgical patterns |

## Structured filters (each = one indexed column/join; AND-composed)

| Flag | Column/join | Notes |
|---|---|---|
| `--repo <name|identity>` | repos.identity | default: current repo when cwd is inside a registered worktree, else all (D6) |
| `--worktree <path>` | worktrees | "this checkout only" |
| `--path <glob>` | worktree_files.path | glob→LIKE/regex |
| `--lang <family>` | elements (scanner-stamped) | |
| `--kind` / `--subkind` | elements | closed enum / free detail |
| `--tags a,b` | smart_content.tags && | OR within flag, AND with others |
| `--source code\|smart\|conversation\|all` | which space (D3) | default `all` code spaces (raw+smart), conversations opt-in until conv storage lands |
| `--min-score` | similarity floor | exposed (fs2 buried it) |
| `--since/--until`, `--role human\|agent` | conversation dims | no-ops with a warning until conv plan |
| `--limit/--offset` | SQL | honest `total` via count (kills fs2's two bad paginations) |

## Ranking (semantic)

1. Filters narrow candidates IN SQL (beside the index — never post-hoc in app code).
2. Vector leg: cosine over `embeddings_*` (raw + smart rows compete; `match_field` reports which won — fs2's good idea, kept).
3. Text leg: `ts_rank` over the same candidates.
4. **Fuse with reciprocal-rank fusion (RRF)** — one CTE, no reranker service (D1). `--rank vector` opts out to pure cosine.
5. **Span dedup, not parent-penalty** (D2): if a hit's ancestor also hits with overlapping span, keep the higher-scored, note the collapse in meta.

## Result envelope (JSON; the human table renders from it)

```json
{ "meta": { "total": 143, "showing": {"from":0,"count":5}, "mode":"semantic", "rank":"rrf",
            "folders": {"crates/daemon":3,"crates/store":2}, "filters_applied":{...} },
  "results": [ { "address":"el:flowspace3/crates/store/src/lib.rs::migrate",
                 "score":0.83, "match_field":"smart", "kind":"function", "lang":"rust",
                 "span":[90,97], "snippet":"...", "smart":"Runs embedded forward-only …",
                 "tags":["migrations","startup"], "repo":"flowspace3", "path":"crates/store/src/lib.rs" } ] }
```

Lean rows only — full content comes from `get` (fs2's min/max detail split dropped, D4). `meta.folders` kept (agent steering).

## get / tree

- `get el:… [--depth N]` → element + raw + smart + tags + children outline + parent chain.
- `get conv:<guid> --around t42 -10 +20` → windowed turns (PRD 26's navigation, first-class from day one).
- `tree <path|address> [--depth]` → structure; for `conv:` = turn/sub-item outline.
- No k-hop/"more-like-this" in v1 (fs2 didn't have it either; add later behind `related` if wanted).

## Decisions

| # | Decision | Rejected | Why |
|---|---|---|---|
| D1 | Hybrid RRF (vector+tsvector, one CTE) is semantic's default | pure cosine (fs2) / external reranker | two indexed signals for one SQL query; `--rank vector` escape hatch |
| D2 | Span-overlap dedup | fs2 parent-penalty walk + knob | same effect, no tunable, no graph walk |
| D3 | One `--source` axis spans raw/smart/conversations | separate verbs per corpus | conversations are peers, not a bolt-on (Jordan); default excludes conv until storage lands |
| D4 | Lean hit rows; depth only via `get` | fs2 detail=min\|max | search stays fast/cheap; no duplicated content path |
| D5 | JSON-only v1; human rendering later (Jordan 2026-08-26) | human tables now | one output path to get right first; envelope defined in workshop 004 |
| D6 | Bare `search` scopes to the current repo when cwd is inside one | always-global default | principle of least surprise; `--repo all` widens |
| D7 | Addresses (`el:`/`conv:`) are the only id surface | numeric ids in results | stable, human-readable, survive re-parse; internal ids never leak |

## Open questions
1. `--source` naming: is folding raw/smart/conversation into ONE axis right, or should raw-vs-smart be a separate `--space` flag from code-vs-conversation? (sketch: one axis)
2. Regex mode against raw_text in PG vs streaming from worktree files (sketch: PG, since raw_text is inline per workshop 002 OQ2).
