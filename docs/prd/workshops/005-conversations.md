# Workshop 005 — Conversations (req-0024..0027)
**Type**: Schema + Intake + Query Contract · **Date**: 2026-08-27 · **Author**: o-prime, every ruling Jordan's, same-day · **Status**: AUTHORITATIVE
**Consumers**: the conversations plan (this workshop's direct successor), store migrations, daemon intake, CLI/MCP query surface.
**Evidence base**: `scratch/conversations-telemetry-sample.md` (xoxarle, 2026-08-27 — 4 real sessions, 6,429 events, 2,999 tool calls, reproducible collector). Payload numbers below are measured, not guessed.

## Why (the raison d'être, agreed with Jordan 2026-08-27)

Agents spend real money; what they generate is gold, and today it evaporates —
compaction, scrollback, session end (the summarize-lane crash lost its only
evidence exactly this way). Conversations carry the WHY that code cannot:
rejected alternatives, rulings, the debugging trail. Doctrine is **total
recall, selective retrieval**: store every turn, summarize + embed like code,
and let relevance be decided at query time — "that time we solved X" becomes a
query costing hundreds of tokens, not scrollback archaeology. Tokens are the
economy: every retrieval stage is an opt-in to spend more.

## Scope (Jordan ruled 2026-08-27 — binding for v1)

**IN**: tables · daemon append-friendly intake · enrichment through the
EXISTING summarize/embed lanes · CLI/MCP query surface (search scope, windowed
get, tree outline) · a manual `import` verb as the intake's first client (so we
dogfood on our own transcripts immediately).
**OUT**: automatic capture (git-ai/harness hooks watching live sessions —
req-0027's mechanism stays deferred; only the surface ships) · conversation-
level rollup summaries ("no rollups in the first release — let's just get the
conversations in") · thinking blocks (claude transcripts store them empty; an
absent-by-harness field cannot be a contract).

## Tables (ref layer — content layer unchanged)

Conversations slot into workshop 002's three layers: these two tables are REF
layer; turn text flows into the EXISTING content layer (elements → smart_content
→ embeddings) and the job backlog untouched.

```sql
CREATE TABLE conversations (
  guid        UUID PRIMARY KEY,            -- caller-supplied or minted at import
  repo_id     BIGINT REFERENCES repos(id), -- anchor: where this conversation happened
  worktree    TEXT,                        -- anchor: path within/beside the repo
  base_sha    TEXT,                        -- anchor: commit base at conversation start
  title       TEXT,                        -- optional; import may derive from first turn
  started_at  TIMESTAMPTZ NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE turns (
  conversation_id UUID NOT NULL REFERENCES conversations(guid),
  turn_no     INT NOT NULL,               -- dense 1..N; sequence IS the navigation shape
  role        TEXT NOT NULL,              -- human | agent
  source      TEXT NOT NULL,              -- human | peer | system  (measured: peer-injected
                                          -- turns EQUAL human turns in count in a fleet)
  head_sha    TEXT,                       -- repo HEAD at time-of-turn: re-derivability
                                          -- is only real if the state is addressable
  at          TIMESTAMPTZ NOT NULL,
  body        TEXT NOT NULL,              -- the turn's prose, verbatim
  items       JSONB NOT NULL DEFAULT '[]',-- typed sub-items (req-0025): generic, no
                                          -- migration per new kind
  blob_sha    TEXT NOT NULL,              -- content address of the canonical stored form —
                                          -- the bridge into the element/content layer
  PRIMARY KEY (conversation_id, turn_no)
);
```

Sequence is to conversations what hierarchy is to code and sections are to
markdown (req-0026) — `turn_no` is the axis everything navigates on.

## Turn payload (measured — the tool-IO decision IS the payload decision)

Tool traffic is 46.6% of all events; inputs are AS BIG as outputs in aggregate
(write/edit bodies = 1.2MB of the 2.85MB input side). Rulings:

| content | stored | why (measured) |
|---|---|---|
| human + agent prose | **verbatim** | the gold; modest volume |
| pij-injected turns | **verbatim**, `source: peer` | equal to human turns in count; must be distinguishable |
| tool inputs | **verbatim** — EXCEPT write/edit-family: path + byte-length only | the body is the very next commit; storing it twice doubles the input bill for zero search value |
| tool outputs | **head 512B** + `total_bytes` + `truncated` flag + tool name | keeps 62.7% of results whole and every error's first lines (errors front-load) at 35.6% of output bytes |
| thinking | **dropped** | empty in claude transcripts; cannot be a contract |
| binary/base64 spans | **dropped** | none significant in sample |

Net on the sample: ~35% of verbatim size while keeping 100% of prose, 100% of
tool intent, and the head of every result.

**v1.1 named upgrade** (design note only, do not build): promote error-marked
outputs to full storage — ~52% of output bytes are unique one-time evidence
(errors, test output) not re-derivable from anchored state; the head rule
keeps their opening lines, full promotion (~0.7MB on the sample) keeps it all.

## Enrichment (existing lanes; simple summaries, size-gated)

Each turn's canonical stored form is content-hashed into the element chain and
rides the existing summarize + embed lanes — GC three-level semantics, spend
guard, and dedupe apply unchanged (agents repeat themselves; identical tool
outputs share one paid enrichment).

- **Below a size threshold** (plan picks the number; order-of-256B): embed the
  raw text only — a five-word turn does not earn an LLM call; raw is its own
  display form.
- **At/above threshold**: simple per-turn summary (smart_content) + embeddings
  for raw and summary both — the summary is the compressed form search returns
  before the caller commits tokens to a raw fetch.
- The oversize guard (w-embed-oversize, landing now) covers pathological turns.

## Query surface (builds on workshop 003 — activates what it reserved)

003 already pinned the addresses (`conv:<guid>`, `conv:<guid>#t<ord>`), the
`--source` axis (conversations OPT-IN, "default excludes conv until storage
lands" — this workshop is that landing), and reserved `--since/--until`,
`--role` as no-op dims. This plan turns them on:

- `search --source conversation` (or `all`): hits are turns —
  `(conversation_id, turn_no)` + one-line context, cheap pointers. NEVER
  blended into code results by default: conversations are opinions at a point
  in time, code is current truth; recency leans higher in ranking than for code.
- `get conv:<guid>#t<ord> --before 10 --after 20`: the contiguous window
  around a hit — the caller picks the numbers, pays only for what it fetches.
- `tree conv:<guid>`: the turn outline (role, source, timestamp, first-line).
- Anchor filters compose: `--repo`, `--path`-adjacent conversation lookup via
  the anchor columns ("conversations about this repo as it was then").

## Intake (surface now, automation later)

One daemon endpoint, append-friendly per req-0027: accept a conversation
header + a batch of turns (idempotent on `(conversation_id, turn_no)` so
re-imports are safe), enqueue enrichment, done. First client is
`flowspace3 conversation import <file|stdin>` — hand-fed transcripts prove
search + windowing on real data with zero live-capture machinery. The live
git-ai/harness submitter is a SEPARATE future packet against this same
endpoint.

## Decisions

| id | decision | rejected | why |
|---|---|---|---|
| C1 | Turns are elements; enrichment via existing lanes | parallel conversation pipeline | one engine, one GC, one spend guard; conversations are a content type, not a product |
| C2 | Tool outputs: 512B head + total_bytes | verbatim / drop entirely | measured: 63% whole + error heads at 36% of bytes; half of output bytes ARE evidence, so v1.1 promotion path is named |
| C3 | Write-family inputs: path + length only | verbatim inputs | the body is the next commit; measured to halve the input bill |
| C4 | No rollup summaries in v1 | conversation-level summaries | Jordan 2026-08-27: "just get the conversations in"; rollups are additive later |
| C5 | Import verb ships in v1; live capture does not | build the git-ai hook now | dogfood immediately on our own transcripts; capture is its own packet against the same endpoint |
| C6 | Per-turn `head_sha` + per-conversation anchor | conversation-level anchor only | truncated outputs are honest only if the state they came from is addressable |
| C7 | Thinking dropped in v1 | store-when-present | absent in claude data; a sometimes-field poisons the contract |
| C8 | `source` field distinguishes human/peer/system | role-only | measured: peer-injected turns equal human turns in an orchestrated fleet |

## Open questions (for the plan, not blockers)

1. Summary size-gate threshold (sketch: ~256B) and whether the 512B output
   head is config or constant (sketch: constant until someone needs it).
2. `remove`/GC interplay: does removing a repo remove its anchored
   conversations, or do conversations outlive the repo? (sketch: outlive —
   the anchor is a pointer, not ownership; GC of their elements follows the
   normal three-level rules.)
3. Import format(s): our own JSONL shape first; adapters for claude/omp
   transcript dialects live in the importer, never in the schema.
