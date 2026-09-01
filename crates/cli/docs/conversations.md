# Conversations: storing the WHY, and asking for it later

Agents spend real money, and what they generate is gold that evaporates —
compaction, scrollback, session end. Code records WHAT was decided;
conversations record why, what was rejected, and how the bug was actually
found. `flowspace3` indexes them as first-class content: turn by turn,
summarised and embedded like code, searchable by meaning.

The doctrine is **total recall, selective retrieval**. Store every turn; let
relevance be decided at query time. "That time we solved X" becomes a query
costing hundreds of tokens instead of scrollback archaeology.

## The loop

```bash
# 1. Store a transcript, or pull one from a native harness session.
flowspace3 conversation import ./session.jsonl
flowspace3 conversation ingest --harness omp --session <id>

# 2. Confirm that the native session delivered at least one indexed turn.
flowspace3 conversation verify --harness omp --session <id>

# 3. Ask one transcript directly. Short/full guids and conv: addresses work.
flowspace3 ask "what did we decide about the foreign key" --conversation <guid>

# 4. Or search discussion broadly across the repository.
flowspace3 search "why did we drop the foreign key" --source conversation

# 5. Read around a search hit. You choose how much you pay for.
flowspace3 get conv:<guid>#t42 --before 10 --after 20

# 6. Or browse the whole thing first.
flowspace3 tree conv:<guid>
```

`conversation list` shows what is indexed; `conversation remove <guid>` forgets
one.

## Verify delivery

`conversation verify` derives the guid with the same `conversation_guid()` code
used by ingest, then queries that exact guid across the whole index. Clients do
not copy the digest layout and cwd cannot narrow the answer. The two identity
forms are mutually exclusive:

```bash
flowspace3 conversation verify --harness <claude|omp|pij|metrics-db> --session <id>
flowspace3 conversation verify --pij <legacy-seat>
```

Success is exit 0 with `ok: true` and
`data {guid,address,turns,repo,worktree,last_turn_at}`. A missing conversation or
a header with zero turns exits non-zero with
`FS3-E-QUERY-CONVERSATION-NOT-FOUND`; the latter also carries
`details.turns: 0`. The command has no `--repo` or `--path` flag, so a consumer
cannot accidentally turn "outside my cwd" into "not delivered".

`--pij` uses the existing `pij sessions` join. That join is legacy-only. An rs
seat absent from it is refused with a message naming `pij req-0033`; use the
native `--harness`/`--session` form when that identity is available.

## Conversations in the default search

The default search ranks code, documents, and conversation turns together.
Conversation rows appear only when they earn their score; `data.composition`
still reports threshold-matching conversation totals below the returned top-k.
Use `--source conversation` when the question asks only for prior discussion,
or `--source code` when only current implementation may answer.

`ask` accepts the same `--source code|doc|conversation|all` axis. Add
`--conversation <guid-or-conv:address>` when the question is about one session:
every retrieval and citation is then hard-bound to that transcript, and the
coverage envelope names its stored turn count. A canonical full guid or `conv:`
address resolves index-wide regardless of cwd, and the tool search/read scope
follows that resolved transcript even when it is foreign or unanchored. An
explicit `--repo` still filters it, while a short prefix remains scoped for
disambiguation. An unknown guid is refused before the chat model runs;
`conversation list` is the authoritative way to choose one.

Do not use `--path` for transcript questions: conversations carry repository and
worktree anchors, not file paths. Use `--conversation <guid>` to pin one transcript
or `--repo <identity>` to scope conversations by repository. `ask --path` with
`--source conversation` is refused before the chat model runs.

## The transcript format

JSONL. An optional header on the first line, then one turn per line:

```jsonl
{"guid":"6ba7b810-9dad-11d1-80b4-00c04fd430c8","title":"the gc incident","repo_identity":"git:github.com/you/repo"}
{"role":"user","content":"why is gc eating my embed jobs"}
{"role":"assistant","content":"because embed payloads carry items, not raw_hash","items":[{"kind":"tool_result","tool":"bash","head":"Reclaimed { jobs: 1 }","total_bytes":24,"truncated":false}]}
```

Everything except the prose is optional:

| field | default |
|---|---|
| `guid` | derived from the file name and its first turn — so a re-import of the same file finds the same conversation |
| `turn_no` | the line's position, dense from 1 |
| `role` | `agent`; `user` and `human` both mean `human` |
| `source` | `human` for a human turn, `system` otherwise — pass `peer` for an agent-injected one |
| `at` | import time |
| `title` | the first line of the first turn |
| anchor | the repository you are standing in |

`content`, `text`, `body` and `message` are all read as the prose. Other
dialects (claude, omp) are translated in the importer and never in the schema.

## Growing a conversation

Re-importing a file that has grown stores only the new turns and enqueues
enrichment only for those. That works because intake is idempotent on
`(conversation_id, turn_no)` — so the loop of "import as you go" costs what it
should.

It only holds if the same guid is used each time. A file that carries its own
`guid` is safe; so is one whose name and first turn are unchanged. Pass
`--guid` when neither is true.

## What is stored, and what is not

Tool traffic is 46.6% of events in a measured fleet, and inputs are as large as
outputs. So (workshop 005):

| content | stored |
|---|---|
| human and agent prose | verbatim — it is the gold, and it is small |
| peer-injected turns | verbatim, marked `source: peer` — measured equal to human turns in count |
| tool inputs | verbatim, EXCEPT write/edit-family: path and byte length only |
| tool outputs | first 512 bytes, plus `total_bytes` and `truncated` |
| thinking blocks | dropped |

The write-family rule is not squeamishness: the body is the very next commit,
so storing it here doubles the input bill for zero search value. The output
head keeps 62.7% of results whole and the opening lines of every error — errors
front-load — at 36% of the bytes.

**The policy is enforced at intake, not trusted from the client.** An importer
that forgets to shape a payload is an ordinary bug; a store that believed it
would be a permanent one.

## The anchor is a pointer, not ownership

A conversation records the repository, checkout and base commit it happened in.
It does not BELONG to them:

- `flowspace3 remove` of the anchored repository leaves the conversation whole.
- Re-adding that repository re-links the anchor automatically — the anchor is
  the repository IDENTITY, not a row id.
- A conversation can be anchored to a repository fs3 has never indexed.

Explicit `get conv:<guid>` and `tree conv:<guid>` addresses follow the pointer
across the entire index instead of inheriting cwd scope. Pass `--repo` only when
you deliberately want the anchor repository to be part of the lookup.

`--repo` and `--path` compose with `--source conversation`, and reach
conversations through their anchor rather than through a live file path.

## Enrichment, and what it costs

Turns ride the same engine as code: one summariser, one embedder, one spend
guard, one collector. Two consequences worth knowing:

- **Identical turns are paid for once.** Enrichment is keyed by content, so the
  same words in forty conversations are one summary and one pair of vectors.
  Agents repeat themselves constantly; this is most of the saving.
- **Small turns are not summarised.** Below `indexing.turn_summary_min_bytes`
  (default 256) a turn is embedded raw and never sent to an LLM. A five-word
  turn does not earn a chat call, and its raw text is already its own display
  form.

Anchored conversations use whatever provider their repository selected;
unanchored ones use the default, through a reserved `conv:unanchored` identity.
Configure a `[repos."conv:unanchored"]` entry to point them somewhere cheaper.

**One caveat, stated rather than hidden:** vectors are only comparable within
one model's space. A repository configured with a non-default embedder has
conversations embedded in THAT space, so they are not cosine-comparable with
the default one. This is already true of code across repositories; it is not
new, but it is worth knowing before you configure a per-repo embedder.

## Garbage collection

A stored turn is a ROOT of reference. An imported conversation has no
registered worktree and never will, so without that rule the first GC pass
would reclaim every turn element and everything the import paid for — silently,
because an empty search result looks exactly like "no match".

`conversation remove` deletes the conversation, its turns and its turn
elements, and stops there. The summaries and vectors they paid for are keyed by
content and may still be shared, so `gc` decides those on its own cadence and
reclaims whatever nothing else carries.

## What v1 does not do

- **Conversation-level rollup summaries.** Per-turn summaries only.
- **Thinking blocks.** Claude transcripts store them empty; an absent-by-harness
  field cannot be a contract.
