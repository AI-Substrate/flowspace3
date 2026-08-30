# search

```bash
flowspace3 search "how does the queue avoid two workers taking the same job"
```

Search runs two legs together: indexed verbatim text and vector similarity.
Exact text hits are pinned first; semantic hits follow in similarity order.

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
  "score": 1.0, "channel": "both", "match_field": "exact_name",
  "kind": "function", "subkind": "function_item",
  "name": "validate_session_token", "span": [42, 58],
  "path": "src/auth.rs", "repo": "git:github.com/org/repo",
  "worktree": "/srv/code/repo", "snippet": "…", "smart": null, "tags": [] }
```

- `path` + `span` is what you open; `span` is inclusive, 1-based.
- `worktree` names the registered checkout that supplied the hit. It is the
  absolute root; `path` is relative to it.
- `score` is channel-native. Semantic rows use `1 - cosine distance`. Exact
  lexical rows use `1.0` because the substring is identical, not because a
  vector was identical.
- `channel` is `lexical`, `semantic`, or `both`. A `both` row keeps lexical
  placement and score; the values are never averaged.
- `match_field` explains the winning evidence: `raw`/`smart` for semantic,
  `exact_name`/`exact_text` for lexical. A `smart` hit found the answer by
  meaning, so the words you typed may appear nowhere in the code.
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
- **The result cap is measured, not implied.** `meta.truncation` reports the
  requested `limit` and whether at least one additional hit existed. Search
  fetches one extra candidate only to establish that fact; `data.results`
  still contains at most the requested limit.
- **A weak match teaches without changing the answer.** When results exist but
  the best score is below the named confidence floor, `meta.hint` and
  `next_action` say: "Weak match: describe the component in its own vocabulary
  rather than asking a question." The hint never filters or reorders results.
  Zero results keep their existing diagnostic steer.
- **Empty results are a real answer — but the surface will tell you when they
  are not.** `meta.empty_because` carries the reason whenever one is known:
  `below_floor` means rows were found and your `--min-score` rejected them, and
  names the floor; `scan_incomplete` means content IS indexed under this scope
  and the ranking stopped before reaching it, which is what a narrow scope over
  a large index does; `path_unmatched` means the requested `--path` glob
  matches zero indexed paths in the scope. That last reason includes a `hint`
  naming indexed top-level entries so the glob can be corrected without
  treating an unsatisfiable filter as code absence. A scope holding nothing at
  all is an error (`FS3-E-QUERY-NO-INDEX`) naming the anchor and pointing at
  `flowspace3 add`, rather than an empty list you would read as a fact about
  your code. With no `empty_because`, check the boring causes: indexing may
  still be running (`flowspace3 status`), or **the active embedder may not be
  the one that built the index** — that one looks exactly like a broken search,
  because vectors are only read under the `model_key` that wrote them.
  `flowspace3 doctor` names the active providers.
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
the registered checkout containing that directory (workshop 003 D6). Content
that exists only in another checkout is excluded before ranking, including
another checkout of the same repository; byte-identical content shared by both
remains visible. Every hit names the serving checkout in `worktree`.

`meta.scope` says which repository and worktree answered and how the scope was
chosen; `--repo all` explicitly widens back to every indexed repository.
Standing somewhere fs3 has never indexed puts a warning in `scope.warnings` and
at the front of `next_action`, naming `flowspace3 add <path>` — rather than
answering from an unrelated repository and letting you believe it was yours.

## Weak-match calibration

The advisory floor is the named `WEAK_MATCH_SCORE_FLOOR` constant beside its
calibration table in `crates/daemon/src/search.rs`. The 2026-08-28 snapshot used
the live repository index and Azure `text-embedding-3-small-no-rate` at 1024
dimensions: known noise topped out at 0.4644 and known-relevant results bottomed
out at 0.5509, so 0.50 sits between the observed bands. The index grows and
absolute similarity is provider-dependent; re-run the labelled query table
before changing the constant. A newly relevant result below the band moves the
floor down. The procedure is durable; the sample is not.

## Depth comes from `get`

Hits are lean on purpose. `flowspace3 get <address>` returns the whole element
(or whole file), and `flowspace3 tree <address-or-path>` browses the structure
around it: `flowspace3 docs get read`.

Regex mode remains out of scope; every ordinary search is fused lexical + semantic.
