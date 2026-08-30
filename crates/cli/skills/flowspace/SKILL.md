---
name: flowspace
description: Use flowspace3 as a semantic search tool over the central code index — detect it, search by meaning, read the JSON envelope, follow up on el: addresses, and use `flowspace3 ask` when a question needs an ANSWER assembled across several places rather than a list of hits. Use when locating where something happens by meaning rather than exact text, when asking how or why something works, or when asked to search the flowspace index.
---

# flowspace — search code by meaning

`flowspace3` splits codebases into elements (functions, types, markdown sections),
summarises and embeds them, and answers questions by meaning across every indexed
repo at once. This skill is the pointer layer: the binary's bundled docs are the
authoritative guides and outlive this page — where the two disagree, the binary wins.

## Output contract for agents

A real TTY (terminal) gets human-readable output. A pipe, file, CI capture, or agent
subprocess keeps receiving the JSON envelope with no flag. `--json` forces JSON
anywhere. A harness running inside a PTY such as tmux looks human to a terminal
probe, so export `FS3_OUTPUT=json` once to pin the machine shape.

## 1. Detect

```bash
command -v flowspace3     # installed at all?
flowspace3 doctor         # stack health; verdict ok|degraded
flowspace3 status         # registered roots + queue state
```

- `status` `roots` answers "is THIS repo indexed?" — your repo's git identity must be
  present. A non-empty `queue` means indexing is still draining: poll until empty
  before concluding a search "found nothing".
- `doctor`'s verdict `degraded` (rather than `ok`) names a stack running on
  stand-in providers — search works, but scores do not mean what they look like (§4).

## 2. Install

Not installed? One line (macOS/Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh
```

then `flowspace3 doctor` — it repairs the whole stack as it goes; there is no second
setup command. The full agent-onboarding funnel lives in the repo README (Install
section) and `flowspace3 docs get install`; this skill deliberately does not restate it.

## 3. Learn

The binary carries its own guides, offline and version-locked to the binary:

```bash
flowspace3 docs list           # every bundled topic
flowspace3 docs get agents     # the operating guide: the loop, the envelope, the gotchas
flowspace3 docs get search     # the query surface: flags, hit shape, ranking
flowspace3 docs get read       # get/tree: addresses, depth, and what scoping means
flowspace3 docs get providers  # registering a real model, from scratch
```

Read `agents` once before anything else; `search` before any non-trivial query;
`providers` before interpreting scores. These pages teach what this skill only points at.

## 4. Search — the heart

```bash
flowspace3 search "how does the queue avoid two workers taking the same job"
```

The query is embedded with the model that embedded the index; the nearest elements
come back ranked. Filters narrow candidates **in SQL, beside the index** — a filter
matching nothing returns nothing, not a padded list:

| flag | effect |
|---|---|
| `--repo <identity>` | one repository, e.g. `git:github.com/org/repo` |
| `--path <glob>` | paths matching a glob (`crates/store/*`) |
| `--limit N` | how many hits (1–100, default 10) |
| `--min-score S` | similarity floor, 0.0–1.0 |
| `--source code\|doc\|conversation\|all` | narrow the corpus; absent/`all` searches every source |

Default search ranks code, document, and conversation rows together. The
`data.composition` counts come from the same `--min-score`-filtered scored set
before top-k truncation, so a conversation below the returned limit is still
visible without changing ranking. Narrow only when the question requires it.

A real answer, trimmed:

```json
{ "ok": true, "command": "search", "v": 1,
  "data": { "results": [
    { "address": "el:git:github.com/AI-Substrate/flowspace3/crates/store/tests/pg_store_flows.rs::a_job_is_claimed_once_and_completing_it_frees_its_key",
      "kind": "function", "match_field": "smart",
      "path": "crates/store/tests/pg_store_flows.rs", "score": 0.55,
      "smart": "Integration test verifies that a queued job is claimed exactly once…",
      "snippet": "async fn a_job_is_claimed_once_and_completing_it_frees_its_key() {…",
      "span": [351, 398], "subkind": "function_item",
      "tags": ["job claiming", "deduplication", "completion"] } ] },
  "next_action": "open a hit at its path and span, or narrow with --path/--repo" }
```

Reading it:

- Branch on `ok` only. `path` + `span` (inclusive, 1-based) is what you open;
  `snippet` is the first few lines only — search returns lean rows on purpose, so
  read the file for the rest.
- `score` = 1 − cosine distance, higher is better. Its meaning depends on the active
  embedder: under the default offline `fake` provider scores are deterministic but
  not semantic — treat them as noise until `doctor` names a real embedder.
- `match_field`: `raw` matched the code; `smart` matched an LLM summary — the answer
  was found by MEANING, your words may appear nowhere in the code. `tags` ride along
  on smart elements.
- `address` (`el:<repo>/<path>::<name>`) is stable across re-parses — the currency
  for follow-ups, and what `get`/`tree` take.
- **A bare search is about the repository you are standing in.** `meta.scope` says
  which one answered; `--repo all` widens. If your directory is not indexed, the
  warning leads `next_action` and names `flowspace3 add <path>` — do not treat hits
  from another repository as yours.
- Empty results are `ok: true` with `"results": []`. **Read `meta.empty_because`
  before rephrasing.** When it is present the surface knows why the list is empty and
  says so: `below_floor` (rows were found and your `--min-score` rejected them),
  `scan_incomplete` (content IS indexed in this scope and the approximate
  nearest-neighbour scan stopped before reaching it — widen with `--repo all`), or
  `path_unmatched` (the glob matches zero indexed paths; read its `hint` for the
  indexed top-level layout and correct `--path`). When it is absent, the `next_action`
  steer names the boring causes instead. A repository with nothing indexed under the
  active model is an ERROR (`FS3-E-QUERY-NO-INDEX`) naming the anchor, not an empty
  answer. Low scores under a real embedder are the same answer with numbers attached.

## 4b. Get and tree — read what you found

```bash
flowspace3 get el:<repo>/<path>::<name>   # that element, in full, with its children outlined
flowspace3 get el:<repo>/<path>           # the whole file, as indexed
flowspace3 tree el:<repo>/<path>          # what that file declares
flowspace3 tree                            # where you are standing, or the index
```

**Search then get, not search then `cat`.** The hit's `address` reads back as real
content out of the index, so you never guess which checkout on disk it came from.
`--depth N` controls the children outline; `--span <line>` picks one element when an
address matches several (`struct Rect` and `impl Rect` share one address by design —
the error lists the candidates and the span to pass).

## 4c. Ask — when you want an answer, not hits

```bash
flowspace3 ask "how does the watcher decide what to rescan?"
```

`search` hands you ranked places to look. `ask` runs a bounded agent loop that does
the looking for you: it searches, reads the addresses that matter, and returns a
written answer citing what it read — or says plainly that it could not find it.

**It is not a faster search. It is a slower, dearer one that thinks.** A search is
one embedding and a query; an ask is many model calls, typically 15–30 seconds and
real tokens. Reach for it when the answer lives in several places at once and you
would otherwise read five files to assemble it. For "where is X handled", search.

### Scope it, or you pay to read the world

**A bare ask is about the repository you are standing in**, exactly like search.
That default is usually right and always cheap.

| flag | effect |
|---|---|
| *(none)* | the repository you are standing in |
| `--repo <identity>` | one named repository |
| `--repo all` | EVERY indexed repository — more searching, more tokens, slower |

Use `--repo all` deliberately, when the answer genuinely crosses repositories
("compare how A and B each do X"), not as a default. Widening multiplies the work on
a many-repo index, and a question whose answer is local gets no better for it.

The loop narrows further on its own — it passes path globs and its own repo argument
to the same filters `search` exposes — so a question phrased with real nouns
("the daemon's job queue", not "the queue") scopes itself.

### Reading the report

```json
{ "ok": true, "command": "ask", "v": 1,
  "data": {
    "answer": "The watcher rescans at directory granularity…",
    "grounded": true,
    "citations": ["el:git:github.com/org/repo/crates/daemon/src/watch.rs::relist"],
    "trace": [ { "iteration": 1, "tool": "search", "failed": false, "evidence": true } ],
    "coverage": { "iterations_used": 5, "iteration_limit": 8,
                  "retrieval_top_k": [6], "exhaustive": false },
    "stopped": "answered", "iterations": 5, "tokens_used": null,
    "model": "gpt-4o" } }
```

- **`citations` is what the loop actually READ**, recorded by the tool layer — not
  the model's own "Sources:" list, which is prose and can be wrong. Verify any claim
  by running `get` on one.
- **`grounded: false` means the answer rests on nothing the loop read.** The loop
  pushes back once and demands evidence before allowing it, so a `false` here is a
  model that insisted. Treat that answer as a guess.
- **Only `ok: true` + `stopped: answered` carries an answer, and that answer is
  always non-empty.** `max_iterations`, `token_budget`, and a provider failure
  after useful reads are `ok: false` terminals with no success `data`. Their
  `error.details.evidence` is explicitly labelled partial and preserves citations
  plus one finding per completed iteration. Use it to narrow a follow-up; never
  present it as the missing answer. For a bound, ask a narrower question or raise
  the matching `[agent]` bound (`token_budget` defaults to 80,000 and is configurable).
- **`coverage` names the probe's finite reach.** `retrieval_top_k` records each search
  cap and `exhaustive` is always false. Enumerations are findings from that bounded
  probe, never proof that the listed items are the only ones.
- **`trace[].evidence`** distinguishes a call that WORKED AND FOUND NOTHING
  (`failed: false, evidence: false`) from one that BROKE (`failed: true`). Both are
  survivable — bad tool calls are fed back to the model, which corrects itself.
- **"I could not find it" is a correct answer**, not a failure. The verb is built to
  prefer an honest not-found over a plausible invention, because a confident wrong
  answer about your own codebase is worse than no answer: you cannot tell it from a
  right one.
- `tokens_used: null` means the provider reported no usage. Null is not zero.

## 5. Why semantic — the judgment

- **The index** answers meaning-shaped questions — "where do we handle X", "how does
  Y work" — and unfamiliar codebases where you cannot name the identifiers yet.
  Use `--source code`, `doc`, or `conversation` only when the question requires one
  corpus; absent/`all` lets relevance rank every source together.
- **Your own grep/ripgrep** answers exact-identifier lookups: a symbol, an error
  string, a literal. Exact text matching is grep's job; do not ask the index to do it.
- **`ask`** answers questions that need assembling — "how does X work", "why is Y
  done this way", "compare how A and B do Z" — where the reply you want is prose with
  citations rather than a list of places. It costs model calls and tens of seconds,
  so it earns its keep on synthesis, not on lookup.
- The question decides: if you already hold an exact string, grep. If you can phrase
  it without knowing any identifier and you want somewhere to look, search. If you
  want the answer itself and it spans several places, ask. When one side comes back
  empty or weak, try the other — that loop, not a rule, is the judgment.

## 6. Failure paths

Every error envelope carries `fix` — the command or config change that resolves it,
not a restatement of what went wrong. **Trust the fix field.**

| symptom | first move |
|---|---|
| Empty results although the index looks full | the active embedder may not be the one that built the index — vectors are only read under the model key that wrote them. Check what is active (`doctor`); if they disagree, re-index: `flowspace3 add .` |
| Daemon down | `FS3-E-DAEMON-UNAVAILABLE` → `flowspace3 daemon &` (doctor diagnoses but never starts one) |
| Repo not indexed | `status` shows no root for it → `flowspace3 add /abs/path` |
| `ask` returned `grounded: false` | it answered without reading anything — treat it as a guess. Check `status` in case nothing is indexed here, then re-ask more narrowly |
| `ask` stopped at a bound | The envelope is `ok:false`; inspect labelled `error.details.evidence`, then ask a narrower question or raise the matching `[agent] max_iterations` / `token_budget` setting. |
| Anything else | `ok: false` → run the `fix`; `retryable: false` means stop and fix something, do not loop |
