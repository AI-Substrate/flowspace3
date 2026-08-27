# fs3 for agents

Semantic code search over a central index. You ask a question in English; you
get back ranked code elements with addresses, paths and line spans.

This page is bundled INSIDE the binary. `flowspace3 docs get agents` prints it
offline, with no daemon and no network.

## The loop

```bash
flowspace3 doctor                  # starts the stack, creates the db, migrates
flowspace3 daemon &                # the indexer; keep it running
flowspace3 add /path/to/repo       # walk, hash, queue
flowspace3 status                  # poll until the queue is empty
flowspace3 search "how does X work"      # find an address
flowspace3 get <address>                 # read what is at it, in full
flowspace3 tree <address-or-path>        # browse what is around it
```

**Search then get** — not search then `cat`. A hit is a lean row with an
address; `get` returns that element's whole text (or a whole file, for a file
address) out of the index, so you never have to guess which checkout on disk
the hit came from. `flowspace3 docs get read` is the detail.

## Storing what you learned

```bash
flowspace3 conversation import ./session.jsonl        # turns become content
flowspace3 search "why did we drop it" --source conversation
flowspace3 get conv:<guid>#t42 --before 10 --after 20 # read around the hit
```

Code records WHAT was decided; a conversation records why, what was rejected,
and how the bug was actually found. Import a transcript and its turns are
indexed like any other content — summarised, embedded, searchable by meaning.
Re-import the same file as it grows and only the new turns land.

Conversations are OPT-IN on search, and that is deliberate: they are opinions
at a point in time and code is current truth, so `search` without
`--source conversation` never blends them in. `flowspace3 docs get
conversations` is the detail.

`doctor` is repair-as-it-goes: it starts the container stack, creates the
database and applies migrations. There is no separate setup step, and you never
need to run `docker compose` yourself.

`daemon` runs in the foreground. It serves HTTP on `daemon.url`, migrates the
store at boot, and drains the job queue. `doctor` reports whether it is running
but never starts one — a diagnostic command must not leave a process behind.

## Finished with a repo

```bash
flowspace3 remove /path/to/repo    # unregister: stop watching, forget its files
flowspace3 gc                      # reclaim what nothing references, now
```

`remove` unregisters a root and kills its queued scans, even mid-index. It does
NOT delete indexed content, and that is deliberate: parses, summaries and
vectors are keyed by CONTENT, so the same file in another registered repo
shares them. Deleting them because one root left would throw away work another
root is still using — and, for summaries, work somebody paid a model for.

What becomes genuinely unreferenced is reclaimed by garbage collection, which
the daemon runs on a slow schedule. `flowspace3 gc` runs a pass now and tells
you what it freed. The `remove` envelope reports what is *reclaimable*, which
is a floor rather than a total — deeper levels only come into view once the
level above is actually collected.

Removing a root you never added is an answer, not an error: the envelope says
so and lists the roots that ARE registered, because paths are stored as the
daemon resolved them (on macOS `/tmp/x` is registered as `/private/tmp/x`).

## First run: you will have no real provider

A fresh install ships ONE provider — a deterministic offline `fake` — and both
ports use it. That is deliberate: it makes the whole stack, search included,
work before you have any credentials.

It also means everything you index is embedded and summarised by a stand-in.
`flowspace3 doctor` reports this as a `providers` warning and the verdict is
`degraded` rather than `ok`.

```bash
flowspace3 doctor                    # the providers row says which are active
flowspace3 docs get providers        # how to register a real one, from scratch
```

If offline is what you want, nothing needs doing — carry on. If you want a real
model, `docs get providers` covers the whole setup: Azure (both auth modes),
OpenAI, and setting the actives globally or per repository.

**If you change the embedder after indexing**, existing vectors were written
under the old model's key and are no longer searched — the index looks full and
search returns nothing. Re-run `flowspace3 add <path>` to re-index.

## Every command answers one envelope

```json
{ "ok": true, "command": "search", "v": 1, "data": { }, "next_action": "…" }
```

```json
{ "ok": false, "command": "search", "v": 1,
  "error": { "code": "FS3-E-STORE-UNAVAILABLE", "message": "…",
             "fix": "…", "retryable": true } }
```

Three rules that make this worth parsing:

- **`ok` is the ONLY discriminator.** Never sniff for the presence of `data` or
  `error`. Branch on `ok`.
- **`error.fix` is mandatory and tells you what to DO** — a command to run or a
  config line to write, not a restatement of what went wrong. If you are
  handling an error, `fix` is the field to act on.
- **`next_action` is a steer, not an instruction.** It says what a consumer
  typically does next. Ignoring it is always safe.

`error.retryable` says whether repeating the same request could succeed without
a change. `false` means stop and fix something.

Exit codes: `0` ok, `1` error, `2` usage.

## Reading a search result

```json
{ "address": "el:git:github.com/org/repo/src/auth.rs::validate_session_token",
  "score": 0.83, "match_field": "smart", "kind": "function",
  "span": [42, 58], "path": "src/auth.rs", "repo": "git:github.com/org/repo",
  "snippet": "…", "smart": "Validates a session token…", "tags": ["auth"] }
```

- `path` + `span` is what you open. `span` is inclusive and 1-based.
- `score` is 1.0 for identical, higher is better.
- `match_field` is `raw` (matched the code) or `smart` (matched an LLM summary
  of it). A `smart` hit means the answer was found by MEANING — the query words
  may appear nowhere in the code.
- `address` is stable across re-parses. Prefer it over line numbers when
  referring to something later.

Useful filters: `--repo <identity>`, `--path <glob>`, `--limit N`,
`--min-score 0.0-1.0`, `--source raw|smart|all`.

**A bare search is about the repository you are standing in** (the CLI sends
your working directory). `meta.scope` reports which repository answered and
why; `--repo all` widens. If you are somewhere that is not indexed,
`scope.warnings` and `next_action` say so and name `flowspace3 add <path>` —
rather than answering, silently, from an unrelated repository.

## Things that will save you a wrong turn

- **Indexing is asynchronous.** `add` returns as soon as the walk is done; the
  work happens in the queue. Poll `flowspace3 status` until nothing is pending
  before concluding that a search "found nothing".
- **Re-running `add` on an unchanged tree costs nothing.** Enrichment is keyed
  by the hash of the text, so a re-scan enqueues zero work. It is safe to call
  repeatedly; it is not a cache you can invalidate.
- **`add` takes a directory** — a git repo, a worktree, or a plain folder. A
  subdirectory is fine and keys to its repository.
- **Pass absolute paths.** A relative path sent to the daemon resolves against
  the DAEMON's working directory, not yours. The CLI absolutises for you; other
  clients must do it themselves.
- **A stale schema is refused, not worked around.** Any command may return
  `FS3-E-STORE-SCHEMA-STALE`; the fix is `flowspace3 doctor`.
- **Nothing is written into the repos you index.** Config lives in
  `~/.config/flowspace3/`; data lives in Postgres.

## Where to look next

`flowspace3 docs list` — every bundled topic.
`flowspace3 docs get search` — the query surface in detail.
`flowspace3 docs get read` — `get`/`tree`, addresses, and scoping.
`flowspace3 docs get doctor` — what doctor checks and repairs.
`flowspace3 docs get providers` — registering a real model, from scratch.
`flowspace3 docs get config` — configuration and its layers.
