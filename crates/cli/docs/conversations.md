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
# 1. Store a transcript. Re-run it as the file grows — only new turns land.
flowspace3 conversation import ./session.jsonl

# 2. Find the moment.
flowspace3 search "why did we drop the foreign key" --source conversation

# 3. Read around the hit. You choose how much you pay for.
flowspace3 get conv:<guid>#t42 --before 10 --after 20

# 4. Or browse the whole thing first.
flowspace3 tree conv:<guid>
```

`conversation list` shows what is indexed; `conversation remove <guid>` forgets
one.

## Conversations are opt-in

`--source conversation` is required. The default search returns code and only
code, and that is deliberate: **conversations are opinions at a point in time,
code is current truth.** Blending them would answer "how does auth work" with
somebody's guess about it from three weeks ago. Turns also lean on recency more
than code does, for the same reason.

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

- **Automatic capture.** The live git-ai/harness submitter is a separate packet
  against the same endpoint. `import` is how transcripts get in today.
- **Conversation-level rollup summaries.** Per-turn summaries only.
- **Thinking blocks.** Claude transcripts store them empty; an absent-by-harness
  field cannot be a contract.
