---
name: flowspace
description: Use flowspace3 to search the central code index by meaning and by keyword — detect it, search, read the JSON envelope, follow up on el: and conv: addresses, and use `flowspace3 ask` when a question needs an ANSWER assembled across several places rather than a list of hits. Use when locating where something happens by meaning rather than exact text, when asking how or why something works, or when asked to search the flowspace index.
---

# flowspace — search code by meaning

`flowspace3` splits codebases into elements (functions, types, markdown sections),
summarises and embeds them, indexes their text, and answers across every indexed repo
and agent conversation. This skill is the pointer layer: the binary's bundled docs are
authoritative and outlive this page — where the two disagree, the binary wins.

## Output contract for agents

A real TTY gets human-readable output. A pipe, file, CI capture, or agent subprocess
gets the JSON envelope with no flag. `--json` forces JSON anywhere. A harness inside a
PTY such as tmux looks human to a terminal probe: export `FS3_OUTPUT=json` once.

## 1. Detect

```bash
command -v flowspace3     # installed at all?
flowspace3 doctor         # stack health, one row per check
flowspace3 status         # registered roots + queue state
```

- `status` `roots` answers "is THIS repo indexed?" — your repo's git identity must be
  present. `queue` rows carry a `state`: only `pending`/`running` mean indexing is still
  draining (wait for them before concluding a search "found nothing"). `failed` rows are
  jobs that gave up — a separate problem (`last_error`), not a backlog.
- `doctor`'s `verdict` is `degraded` when ANY row is `warn` or `down` — a slow `search`
  row is enough. Read the rows, not the verdict; `healthy` (a separate field) is true
  when the walk completed. If the `providers` row names the offline `fake` embedder,
  semantic scores are deterministic noise.

## 2. Install

Not installed? One line (macOS/Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh
```

then `flowspace3 doctor` — it repairs the whole stack as it goes; there is no second
setup command. The full onboarding funnel is `flowspace3 docs get install`.

## 3. Learn

The binary carries its own guides, offline and version-locked to it:

```bash
flowspace3 agents-start-here   # the front door: the agents guide, steered to your next step
flowspace3 docs list           # every topic: agents, install, doctor, daemon, search, ddocs,
                               # read, conversations, config, providers, decisions, prompt-search
flowspace3 docs get search     # flags, hit shape, ranking, every empty/degraded case
```

Read `agents` once before anything else and `search` before any non-trivial query.

## 4. Search — the heart

```bash
flowspace3 search "how does the queue avoid two workers taking the same job"
```

Every search runs two legs together — **keyword** (names and source text) and
**meaning** (embeddings) — and returns one ranking by score. Filters narrow candidates
**in SQL, beside the index**: a filter matching nothing returns nothing, not padding.

| flag | effect |
|---|---|
| `--repo <identity>` | one repository (`git:github.com/org/repo`, or its remote URL); `all` widens to every one |
| `--path <glob>` | paths matching a glob (`crates/store/*`) |
| `--limit N` / `--offset N` | page size (1–100, default 10) and page start |
| `--min-score S` | score floor, 0.0–1.0 |
| `--source code\|doc\|conversation\|all` | narrow the corpus; absent/`all` ranks every source together |
| `--no-balance` | the raw ranking, without per-file/per-conversation balancing |
| `--include-retrieval` | also show turns that are only fs3 search calls and their output |
| `--id-kind`, `--gate-open`/`--gate-closed`, `--ddoc-schema` | deterministic-document rows only (`docs get ddocs`) |

A synthetic answer, trimmed:

```json
{ "ok": true, "command": "search", "v": 1,
  "data": { "results": [
    { "address": "el:git:github.com/org/repo/src/jobs.rs::Queue::claim_next",
      "score": 0.61, "channel": "semantic", "match_field": "smart",
      "kind": "function", "path": "src/jobs.rs", "span": [118, 152],
      "smart": "Claims the oldest pending job under a row lock…",
      "snippet": "/// Take one job, or none.\npub async fn claim_next(…",
      "tags": ["job claiming"] } ],
    "offset": 0, "next_offset": 10,
    "scope": { "repo": "git:github.com/org/repo", "applied": "cwd",
               "other_repo_hits": 0, "outscored": false } },
  "meta": { "empty_because": null, "scan_incomplete": false, "passes": 1,
            "truncation": { "limit": 10, "truncated": true },
            "filtered": { "retrieval_turns": 2 }, "timing": { "total_ms": 412 } },
  "next_action": "next page: `flowspace3 search '…' --limit 10 --offset 10`; …" }
```

Reading a hit:

- Branch on `ok` only. Open `path` + `span` (inclusive, 1-based). A span can start
  above the declaration line: doc comments and attributes are indexed with it.
  `snippet` is the first few lines only — read the rest with `get`.
- `address` is `el:<repo>/<path>::<name>`, with containers nested
  (`::Queue::claim_next`); conversation turns are `conv:<guid>#t<n>` and carry `agent`
  (harness, models, seat). Addresses are stable — the currency for `get`/`tree`.
- `score` is channel-native, higher is better; `channel` is `lexical`, `semantic`, or
  `both` (stronger score plus a tenth of the weaker, capped at 1.0):

| `match_field` | channel | what matched | score |
|---|---|---|---|
| `raw` | semantic | the element's own text, by meaning | cosine similarity |
| `smart` | semantic | its LLM summary — your words may appear nowhere in the code | cosine similarity |
| `exact_name` | lexical | the query, verbatim, in the declaration's name | 1.0 in code/docs |
| `exact_text` | lexical | the query, verbatim, in the source | 1.0 in code/docs, ~0.8 in a turn |
| `terms` | lexical | every term, not as one phrase | ~0.65 (~0.9 if all in the name) |

Keyword matching is case-insensitive and needs every term. Question words (`how`, `why`,
`did`, `where`, ...) are the exception: a hit without them still counts, at ~0.60
instead of ~0.65. Where a term may match
depends on `search.lexical_backend`: `trigram` (default) matches a substring anywhere;
`bm25`, when configured, matches at token starts (`job` finds `jobs`, but `ounded`
does not find `bounded`).

Reading the page:

- **A bare search is about the repository you are standing in.** `meta.scope` says
  which one answered. When other repositories rank in the unscoped top page,
  `meta.elsewhere` counts them and gives the `command` to see them;
  `data.scope.outscored: true` means one of them beat this repository's best hit. Not
  indexed here? `next_action` leads with `flowspace3 add <path>` — do not treat hits
  from elsewhere as yours.
- **Balancing:** by default a page keeps at most one hit per file and two per
  conversation, moving the rest down (never dropped; a 1.0 hit never moves). The kept
  hit says so in `balanced` (`moved_down`, `see_all`); `--no-balance` shows the raw ranking.
- **Keyword-only pages:** `meta.degraded_because: embedder_unavailable` means the
  embedder did not answer and nothing on the page was ranked by meaning — an absence
  there is NOT an answer. `doctor`/`status` show how long it has failed.
- **Paging:** `data.next_offset` appears only when the page filled `--limit`, and
  `next_action` then names the next-page command. `meta.truncation.truncated` says
  whether more ranked hits exist.
- **Hidden turns:** turns that are only fs3 retrieval calls are hidden by default;
  `meta.filtered.retrieval_turns` counts them, `--include-retrieval` shows them.
- **Weak matches:** `meta.hint` appears when the best score is low — describe the
  component in its own vocabulary instead of asking a question.
- `meta.timing` splits where the time went; doctor's `search` row gives p50/p95.

**Empty results are `ok: true`, `"results": []`. Read `meta.empty_because` before
rephrasing.** The field is always present (`null` when no reason is known — then
`next_action` names the boring causes: indexing still running, or an active embedder
other than the one that built the index):

| `empty_because` | means | move |
|---|---|---|
| `below_floor` | rows were found; your `--min-score` rejected them | lower or drop the floor |
| `scope_unmatched` | the scope was compared in full; nothing passed the content filters | drop the filters, or widen the scope |
| `scan_incomplete` | content IS indexed here, but approximate ranking stopped before it — only scopes above `fs3.exact_scope_limit` (150k texts) | narrow with `--path` so the scope is ranked exactly, or drop the scope |
| `path_unmatched` | the `--path` glob matches no indexed path | read `hint` for the real top-level layout; fix the glob |
| `retrieval_hidden` | every match was fs3 retrieval traffic | `--include-retrieval` |

`meta.scan_incomplete` (bool) and `meta.passes` ride on every page. A repository with
nothing indexed under the active model is an ERROR (`FS3-E-QUERY-NO-INDEX`), not an
empty answer.

## 4b. Get, tree, refs — read what you found

```bash
flowspace3 get el:<repo>/<path>::<name>       # that element in full, children outlined
flowspace3 get el:<repo>/<path>               # the whole file, as indexed
flowspace3 get conv:<guid>#t42 --before 5 --after 20   # a window of turns around one
flowspace3 tree el:<repo>/<path>              # what that file declares
flowspace3 refs src/auth.rs                   # ddoc rows that cite a source file
```

**Search then get, not search then `cat`** — the address reads back from the index.
`--depth N` sizes the outline; `--span <line>` picks one element when several share
an address (the error lists candidates).
For a turn, what happened NEXT is usually the point: `--after` defaults to 20.

## 4c. Ask — when you want an answer, not hits

```bash
flowspace3 ask "how does the watcher decide what to rescan?"
```

`ask` runs a bounded agent loop: it searches, reads the addresses that matter, and
returns a written answer citing what it read — or says plainly it could not find it.
**It is not a faster search; it is a slower, dearer one that thinks** (many model
calls, 15–30 s). Use it when the answer lives in several places at once.

| flag | effect |
|---|---|
| *(none)* | the repository you are standing in — usually right, always cheapest |
| `--repo <identity>` / `--repo all` | one named repository / EVERY one (more tokens, slower) |
| `--path <glob>` | hard-bounds every search, read and citation; conversations are excluded |
| `--source code\|doc\|conversation\|all` | one content source |
| `--conversation <guid>` | exactly one transcript; refused with `--path` or `--source code\|doc` |

```json
{ "ok": true, "command": "ask", "v": 1,
  "data": { "answer": "The watcher rescans at directory granularity…",
    "grounded": true,
    "citations": ["el:git:github.com/org/repo/src/watch.rs::Watcher::relist"],
    "trace": [ { "iteration": 1, "tool": "search", "failed": false, "evidence": true } ],
    "coverage": { "iterations_used": 5, "iteration_limit": 8, "retrieval_top_k": [6],
                  "corpus": { "source": "all" }, "exhaustive": false },
    "stopped": "answered", "tokens_used": null } }
```

- **`citations` is what the loop actually READ**, recorded by the tool layer — not the
  model's prose. Verify a claim by running `get` on one.
- **`grounded: false`** — the answer rests on nothing the loop read. Treat it as a guess.
- **Only `ok: true` + `stopped: answered` carries an answer.** `max_iterations`,
  `token_budget` and provider failures are `ok: false`; `error.details.evidence` holds
  labelled partial findings — use them to narrow a follow-up, never as the answer.
- **`coverage`** is the probe's finite reach: `iteration_limit` is `[agent]
  max_iterations` (default 8), `exhaustive` is always false, and a `--path` run adds
  `corpus.path` (glob, element count, why conversations were excluded). An enumeration
  is a finding, never proof the list is complete.
- `trace[]`: `evidence: false` worked and found nothing; `failed: true` broke. "I could
  not find it" is a correct answer. `tokens_used: null` means unreported, not zero.

## 5. Choosing the tool

- **search** — "where do we handle X", unfamiliar code whose identifiers you cannot name
  yet, and identifiers or error strings too: the keyword leg ranks verbatim matches at
  the top. Narrow `--source` only when the question needs one corpus.
- **grep/ripgrep** — an exhaustive list of EVERY occurrence of a literal in a checkout.
  Search returns a ranked, balanced page, not a complete list.
- **ask** — "how does X work", "why is Y done this way", "compare A and B": prose with
  citations, at the cost of model calls.
- **`flowspace3 decision`** — not search: a bounded choice/score/probability judgment on
  text or images. It calls a paid OpenAI model (`docs get decisions`).
- When one comes back empty or weak, try another — that loop is the judgment.

## 6. Failure paths

Every error envelope carries `fix` — the command or config change that resolves it.
**Trust the fix field.**

| symptom | first move |
|---|---|
| Empty results although the index looks full | the active embedder may not be the one that built the index — vectors are read only under the model that wrote them. Compare with `doctor`'s `providers` row; re-index with `flowspace3 add .` if they disagree |
| `degraded_because: embedder_unavailable` | keyword-only page; fix the embedder (`doctor`), then search again before trusting an absence |
| Daemon down | `FS3-E-DAEMON-UNAVAILABLE` → `flowspace3 daemon &` (doctor diagnoses but never starts one) |
| Repo not indexed | `status` shows no root for it → `flowspace3 add /abs/path` |
| `ask` returned `grounded: false` | a guess; check `status` in case nothing is indexed here, then re-ask more narrowly |
| `ask` stopped at a bound | `ok: false`; inspect `error.details.evidence`, ask narrower, or raise `[agent] max_iterations` / `token_budget` |
| Anything else | `ok: false` → run the `fix`; `retryable: false` means stop and fix something, do not loop |
