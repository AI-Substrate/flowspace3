# search

```bash
flowspace3 search "how does the queue avoid two workers taking the same job"
```

Search runs two legs together: a keyword leg over element names and source text,
and vector similarity. Both feed one ranking by score, highest first. Verbatim
matches in code and documents score 1.0, so they lead; every other hit, from
either leg, takes its place by its own score.

## Output shape

A TTY (terminal) receives the human search table. A pipe, file, CI capture, or agent
subprocess receives the JSON envelope with no flag. `--json` forces JSON
anywhere. A harness inside a PTY such as tmux should export `FS3_OUTPUT=json`
because the terminal probe otherwise looks human.

## Flags

| flag | effect |
|---|---|
| `--repo <identity>` | one repository, e.g. `git:github.com/org/repo`; a remote URL (`https://github.com/org/repo.git`, `git@github.com:org/repo.git`, `github.com/org/repo`) names the same repository |
| `--path <glob>` | paths matching a glob (`crates/store/*`) |
| `--limit N` | how many hits (1–100, default 10) |
| `--offset N` | skip the first N ranked hits (default 0) |
| `--min-score S` | similarity floor, 0.0–1.0 |
| `--source code\|doc\|conversation\|all` | narrow the content corpus; absent/`all` searches every source |

Filters narrow candidates **in SQL**, beside the index — not after the fact. A
filter that matches nothing returns nothing rather than a padded list.

Paging is offset-based. `data.offset` always reports the page start;
`data.next_offset` is present only when the returned page filled `--limit`, and
equals `offset + results.length`. A full page therefore offers the exact next
`flowspace3 search … --limit N --offset M` command in `next_action`; an empty
past-end page has no `next_offset`.

`ask` accepts the same `--path <glob>` spelling and matching rules, but binds the
filter for the whole agent run: model tool calls cannot replace it, and citations
outside it are refused. Conversations have no file path, so a path-scoped ask
excludes them; `--path` with `--source conversation` is contradictory.

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
- `score` is channel-native. Semantic rows use `1 - cosine distance`. Lexical
  rows are graded: `1.0` when the whole query occurs verbatim in a code or
  document element (its name or its text), about 0.9 when the declaration's
  name holds every term, about 0.8 for the whole phrase in a conversation
  turn, and about 0.65 when every term occurs but not as one phrase. A 1.0
  means the text is identical, not that a vector was.
- `channel` is `lexical`, `semantic`, or `both`. A `both` row scores the
  stronger of its two scores plus a tenth of the weaker, capped at 1.0:
  corroboration only ever adds.
- `match_field` explains the winning evidence. Semantic: `raw` (the element's
  own text) or `smart` (its LLM summary). Lexical: `exact_name` (the query,
  verbatim, in the declaration's name), `exact_text` (verbatim in the source),
  or `terms` (every term, not as one phrase). A `smart` hit found the answer by
  meaning, so the words you typed may appear nowhere in the code.
- Keyword matching is case-insensitive and needs every term. Question words
  (`how`, `why`, `did`, `where`, ...) are the exception: a hit without them
  still counts, at about 0.60 instead of 0.65. Where a term may
  match depends on `search.lexical_backend`: `trigram` (the default) matches a
  substring anywhere; `bm25`, when configured, matches at token starts (`job`
  finds `jobs`; `ounded` does not find `bounded`).
- `snippet` is the first few lines only. Search returns lean rows on purpose.

## Every source, one ranking

The default corpus includes code declarations, document sections/rows, and
conversation turns. Relevance remains the guard: all sources compete in the
same lexical/semantic ranking and only rows meeting `--min-score` count.
`--source code`, `--source doc`, and `--source conversation` narrow that corpus;
`--source all` is the explicit spelling of the default.

Every element may have a vector of its own text (`raw`) and an LLM-summary
vector (`smart`). Both compete internally, and `match_field` reports which won.

## Things that surprise people

- **A file element its children cover has no vector.** Its text is the
  concatenation of its own functions, so it would out-rank every one of them on
  any question about that file. Files with no parsed children (prose, unknown
  languages) do get one.
- **The result cap is measured, not implied.** `meta.truncation` reports the
  requested `limit` and whether the scored set held more. `data.composition`
  counts code, document, and conversation rows from that same thresholded set
  before the display limit, so a top-k containing only files does not hide a
  relevant conversation below it.
- **Paging keeps rank order stable.** Apply `--offset` after ranking: page 1 and
  page 2 are disjoint slices of the same ordered result set. Equal scores use a
  deterministic element-id tiebreak, so tied rows do not drift between pages.
- **A weak match teaches without changing the answer.** When results exist but
  the best score is below the named confidence floor, `meta.hint` and
  `next_action` say: "Weak match: describe the component in its own vocabulary
  rather than asking a question." The hint never filters or reorders results.
  Zero results keep their existing diagnostic steer.
- **Empty results are a real answer — but the surface will tell you when they
  are not.** `meta.empty_because` is on every page — `null` when no reason is
  known — and carries the reason whenever one is:
  `below_floor` means rows were found and your `--min-score` rejected them, and
  names the floor; `scope_unmatched` means the scope was small enough to
  compare every one of its vectors and nothing in it matched your content
  filters (`--source`, `--id-kind`, `--gate-open`/`--gate-closed`,
  `--ddoc-schema`): an answer about that scope, not a scan that gave up — drop
  the filters, or widen the scope; `scan_incomplete` means content IS indexed
  under this scope and the approximate ranking stopped before reaching it,
  which only a scope above `fs3.exact_scope_limit` (150,000 texts) can hit —
  narrow with `--path` so the scope is ranked exactly, or drop the scope (an
  unscoped search never reports it as the reason: there is no scope to say
  the content is in); `retrieval_hidden` means every match was fs3's own
  retrieval traffic (`--include-retrieval` shows it); `path_unmatched` means
  the requested `--path` glob matches zero indexed paths in the scope. That
  last reason includes a `hint`
  naming indexed top-level entries so the glob can be corrected without
  treating an unsatisfiable filter as code absence. A scope holding nothing at
  all is an error (`FS3-E-QUERY-NO-INDEX`) naming the anchor and pointing at
  `flowspace3 add`, rather than an empty list you would read as a fact about
  your code. With no `empty_because`, check the boring causes: indexing may
  still be running (`flowspace3 status`), or **the active embedder may not be
  the one that built the index** — that one looks exactly like a broken search,
  because vectors are only read under the `model_key` that wrote them.
  `flowspace3 doctor` names the active providers.
- **One conversation or file cannot fill a page.** A page keeps at most two
  hits per conversation and one per file, in rank order, and moves the rest
  below; nothing is dropped, so later pages still reach them. The top kept hit
  of a conversation or file that lost hits carries `balanced` (`by`,
  `moved_down`, and `see_all`: this same page with `--no-balance`), and
  `meta.balanced` totals them. An identifier-exact hit (score 1.0) never
  moves. A search inside one conversation (`ask --conversation`) is never
  balanced, nor is a page where only one conversation (or one file) answered.
  `--no-balance` returns the raw ranking.
- **A search answers even when the embedder does not.** The query is embedded
  within 5 seconds or not at all. When the embedder fails or does not answer
  (an expired login, an unreachable token endpoint), the search returns its
  keyword matches only, says so in `meta.degraded_because`
  (`embedder_unavailable`) and `meta.degraded` (`detail`, `since`), and leads
  `next_action` with it. Nothing on that page was ranked by meaning, so an
  absence there is not an answer. `flowspace3 status` and `flowspace3 doctor`
  show how long the embedder has been failing.
- **A small scope is searched exactly; a large one approximately.** When
  `--repo`, `--path`, a worktree or a conversation narrows the search to at
  most 150,000 distinct texts, fs3 compares the query with every vector in that
  scope: the hits are the true nearest, and an empty page means the scope holds
  no match (`meta.passes` is 1). A larger scope, and an unscoped search, use
  the HNSW index, which examines a bounded candidate set (pgvector's
  `hnsw.max_scan_tuples`, 20,000 by default) and keeps scanning until your
  `--limit` is filled. The ordering there is nearest-so-far, not provably the
  global nearest, and an approximate page that comes back short reports
  `scan_incomplete` rather than claiming the scope ran out. `meta.scan_incomplete`
  (a boolean) and `meta.passes` ride on every page, empty or not. The cut-off is
  `fs3.exact_scope_limit`, settable per database
  (`ALTER DATABASE … SET fs3.exact_scope_limit = N`); under a lower one, a
  scope whose approximate page comes back short is ranked exactly anyway, up
  to 150,000.
  An exact ranking reads every scope vector in full by default. With
  `fs3.exact_rank = 'compact'` (per database, after `flowspace3 doctor
  exact-index` has filled the copy) it ranks the scope on each vector's
  128-byte binary copy and re-ranks the nearest few windows by the full vector
  (`fs3.exact_rerank`, default 8): the same top ten in measured practice, at a
  third of the reads. `meta.timing.semantic_rank` says which ranking ran:
  `ann`, `exact` or `compact`.
- **Content filters narrow the scope before ranking.** In a scoped search,
  `--source`, `--id-kind`, `--gate-open`/`--gate-closed` and `--ddoc-schema`
  choose which of the scope's texts are candidates at all, so a doc search in a repository
  whose nearest content is conversation or code still reaches its docs. The
  same filters on an unscoped search are applied to the nearest candidates,
  which is why a narrow filter over the whole index can still come back short.
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

Repository identities are stored in one form, `git:<host>/<owner>/<name>`, the
key a checkout of that remote gets. Conversations whose transcript recorded the
remote as a URL are stored under the same key (and older rows are rewritten
when the daemon starts), so `--repo` with either spelling finds both the code
and the conversations about it. A repository known only through conversations,
with no checkout added, is a valid `--repo` too.
Standing somewhere fs3 has never indexed puts a warning in `scope.warnings` and
at the front of `next_action`, naming `flowspace3 add <path>` — rather than
answering from an unrelated repository and letting you believe it was yours.

A scoped search also tells you what it did NOT show. On the first page of a
search scoped by your working directory, the same query is ranked across every
repository alongside it, and when that top page holds hits from other
repositories, `meta.elsewhere` counts them (`hits`, `of_top`, per-repository
`repos`, and the `command` to see them) and `next_action` leads with them:
"this searched only the repository you are in; the top 10 across all
repositories include 6 from others (pij 5, chainglass 1)". A named `--repo`
never carries it — that scope was asked for exactly.

The scope is in `data` too, beside the results it shaped, so a reader of
`data` alone cannot mistake a scoped miss for broken recall. `data.scope`
carries `repo`, `applied` (`cwd`, `explicit` for `--repo`, or `all`),
`other_repo_hits` (the count above; `null` when it was not measured: a named
repository, an unscoped search, or a later page) and `outscored`. `outscored`
is true when a hit in another repository scores higher than the best hit here,
or when this scope found nothing and others did. Then `scope.warnings` and the
front of `next_action` also say "hits in other repositories outscore this
repository's best".

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
