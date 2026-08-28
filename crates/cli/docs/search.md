# search

```bash
flowspace3 search "how does the queue avoid two workers taking the same job"
```

Semantic search: the query is embedded with the same model that embedded the
index, and the nearest elements come back ranked.

## Output shape

A TTY (terminal) receives the human search table. A pipe, file, CI capture, or agent
subprocess receives the JSON envelope with no flag. `--json` forces JSON
anywhere. A harness inside a PTY such as tmux should export `FS3_OUTPUT=json`
because the terminal probe otherwise looks human.

## Flags

| flag | effect |
|---|---|
| `--repo <identity>` | one repository, e.g. `git:github.com/org/repo` |
| `--path <glob>` | paths matching a glob (`crates/store/*`) |
| `--limit N` | how many hits (1–100, default 10) |
| `--min-score S` | similarity floor, 0.0–1.0 |
| `--source raw\|smart\|all` | which vector space to search |

Filters narrow candidates **in SQL**, beside the index — not after the fact. A
filter that matches nothing returns nothing rather than a padded list.

## Reading a hit

```json
{ "address": "el:git:github.com/org/repo/src/auth.rs::validate_session_token",
  "score": 0.83, "match_field": "smart", "kind": "function",
  "subkind": "function_item", "name": "validate_session_token",
  "span": [42, 58], "path": "src/auth.rs", "repo": "git:github.com/org/repo",
  "snippet": "…", "smart": "Validates a session token…", "tags": ["auth"] }
```

- `path` + `span` is what you open; `span` is inclusive, 1-based.
- `score` is `1 - cosine distance`: 1.0 is identical, higher is better. That
  conversion happens once, at the boundary, so `--min-score 0.7` means what it
  looks like.
- `match_field` is `raw` or `smart`. **A `smart` hit found the answer by
  meaning** — the words you typed may appear nowhere in the code. This is why
  "why is enrichment keyed by a hash" can find the right function.
- `snippet` is the first few lines only. Search returns lean rows on purpose.

## Two spaces, one table

Every element gets a vector of its own text (`raw`). Elements above the summary
line floor also get an LLM summary, and that summary gets its own vector
(`smart`). Both compete in the same ranking and `match_field` reports which
won.

`--source smart` searches only summaries — good for conceptual questions.
`--source raw` searches only code — good when you know roughly what the code
says.

## Things that surprise people

- **A file element its children cover has no vector.** Its text is the
  concatenation of its own functions, so it would out-rank every one of them on
  any question about that file. Files with no parsed children (prose, unknown
  languages) do get one.
- **Empty results are a real answer — but the surface will tell you when they
  are not.** `meta.empty_because` carries the reason whenever one is known:
  `below_floor` means rows were found and your `--min-score` rejected them, and
  names the floor; `scan_incomplete` means content IS indexed under this scope
  and the ranking stopped before reaching it, which is what a narrow scope over
  a large index does. A scope holding nothing at all is an error
  (`FS3-E-QUERY-NO-INDEX`) naming the anchor and pointing at `flowspace3 add`,
  rather than an empty list you would read as a fact about your code. With no
  `empty_because`, check the boring causes: indexing may still be running
  (`flowspace3 status`), or **the active embedder may not be the one that built
  the index** — that one looks exactly like a broken search, because vectors
  are only read under the `model_key` that wrote them. `flowspace3 doctor`
  names the active providers.
- **Ranking is approximate, and narrowing costs recall.** The similarity index
  is HNSW: it examines a bounded candidate set rather than every vector, and
  filters apply to what it examined. fs3 keeps scanning until your `--limit` is
  filled, so a scoped search returns as many hits as it was asked for — but the
  ordering is still nearest-so-far, not provably the global nearest. Dropping
  `--repo`/`--path` searches a bigger candidate pool.
- **Vectors are only comparable within one model.** Changing the embedding
  model means a new `model_key`; old rows survive but are not searched by the
  new one. Re-index to move them.

## Scope: a bare search is about where you are

The CLI sends your working directory, and a search with no `--repo` narrows to
the repository that directory belongs to (workshop 003 D6). `meta.scope` says
which repository answered and how it was chosen; `--repo all` widens back to
every indexed repository. Standing somewhere fs3 has never indexed puts a
warning in `scope.warnings` and at the front of `next_action`, naming
`flowspace3 add <path>` — rather than answering from an unrelated repository
and letting you believe it was yours.

## Depth comes from `get`

Hits are lean on purpose. `flowspace3 get <address>` returns the whole element
(or whole file), and `flowspace3 tree <address-or-path>` browses the structure
around it: `flowspace3 docs get read`.

Not in this version: text and regex modes, hybrid ranking.
