---
name: flowspace
description: Use flowspace3 as a semantic search tool over the central code index — detect it, ask meaning-shaped questions of indexed code, read the JSON envelope, follow up on el: addresses. Use when locating where something happens by meaning rather than exact text, or when asked to search the flowspace index.
---

# flowspace — search code by meaning

`flowspace3` splits codebases into elements (functions, types, markdown sections),
summarises and embeds them, and answers questions by meaning across every indexed
repo at once. This skill is the pointer layer: the binary's bundled docs are the
authoritative guides and outlive this page — where the two disagree, the binary wins.

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
| `--source raw\|smart\|all` | which vector space: code text / LLM summaries / both |

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
- Empty results are `ok: true` with `"results": []` and a `next_action` steer (widen
  the query, drop `--min-score`, check `status`). Low scores under a real embedder
  are the same answer with numbers attached.

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

## 5. Why semantic — the judgment

- **The index** answers meaning-shaped questions — "where do we handle X", "how does
  Y work" — and unfamiliar codebases where you cannot name the identifiers yet.
  `--source smart` for conceptual questions; `--source raw` when you know roughly
  what the code says.
- **Your own grep/ripgrep** answers exact-identifier lookups: a symbol, an error
  string, a literal. Exact text matching is grep's job; do not ask the index to do it.
- The question decides: if you can phrase it without knowing any identifier, ask the
  index; if you already hold an exact string, grep. When one side comes back empty or
  weak, try the other — that loop, not a rule, is the judgment.

## 6. Failure paths

Every error envelope carries `fix` — the command or config change that resolves it,
not a restatement of what went wrong. **Trust the fix field.**

| symptom | first move |
|---|---|
| Empty results although the index looks full | the active embedder may not be the one that built the index — vectors are only read under the model key that wrote them. Check what is active (`doctor`); if they disagree, re-index: `flowspace3 add .` |
| Daemon down | `FS3-E-DAEMON-UNAVAILABLE` → `flowspace3 daemon &` (doctor diagnoses but never starts one) |
| Repo not indexed | `status` shows no root for it → `flowspace3 add /abs/path` |
| Anything else | `ok: false` → run the `fix`; `retryable: false` means stop and fix something, do not loop |
