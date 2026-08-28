# convo-source-claude — the Claude Code native reader

**Unit** u1a of plan 005-convo-ingest · **Port** `fs3_core::ConversationSource`
· **Module** `fs3_providers::conversation_sources::claude`

Reads Claude Code's native session store — the session jsonl, its subagent
sidecars, and its spilled tool results — into `RawRecord`s with byte-offset
cursors, so a second ingest of a session that grew costs only the turns that
are new.

---

## FROZEN: how an ordinal is derived

> **The ordinal of a turn is the `uuid` of the FIRST jsonl line of its merged
> group, verbatim, as the store spelled it.** For an unmerged record it is that
> record's own `uuid`.

This is a **persisted contract, not an implementation detail.** The ordinal is
the key the cursor-state service deduplicates on, and it is written to Postgres
where it outlives every process.

**If you change how this is derived — a different field, a different rendering,
first-of-group becoming last-of-group — every record already stored looks brand
new on the next poll and every affected conversation SILENTLY DOUBLES.** There
is no clean recovery: forgetting the session re-reads from zero and duplicates
anyway. The damage is silent, permanent, and proportional to how long the store
has been running.

If you believe this derivation is wrong, that is a message to the plan's PM
before it ships and a plan after. It is not a local edit, and it is not
cleanup.

Why *first* and not last: it is stable under a rescan. A full re-read regroups
the same blocks and computes the same first uuid, so the ledger recognises the
record it already stored. A group's last uuid changes as the group grows, which
would defeat the dedupe precisely when a rotation forced it to matter.

Why the line `uuid` and not `message.id`: `fs3_testkit::Expectations` holds every
reader to emitting an in-order, repeat-free subsequence of the ids the store
actually wrote, and those are the per-line uuids. `message.id` appears nowhere
in that set, so it would fail on every record.

**A record with no `uuid` is REFUSED, never given a placeholder.** `Line::uuid`
is deliberately not an `Option`, so serde drops such a line exactly as it drops
any other unreadable one. A defaulted empty ordinal would not be cosmetic: two
of them collide, so the ledger stores the first and treats every later one as
already seen — in that poll and in every future poll, because the placeholder is
by then a durable ledger row — and real turns would be lost silently and
permanently. Dropping a record that could never be addressed is recoverable;
poisoning the dedupe key is not. (Found by u2 reviewing all four derivations,
F-A1; pinned by `a_record_without_a_uuid_is_refused_not_given_an_empty_ordinal`,
which was verified against the defaulting shape and sees `["ok-1", "", "",
"ok-2"]` when it regresses.)

## FROZEN: the GROUPING RULE, not just the datum

The derivation above is *first-uuid-of-**group***, so it depends on the rule
that decides group membership every bit as much as on the uuid. That rule is
frozen too, and carries the same consequence:

> **A group is every record of type `assistant` sharing one `message.id`.
> Membership is decided by RECORD TYPE and `message.id` ALONE — never by which
> content blocks a record holds, and never by payload policy.**

Widen the set of merged record types, merge on a different key, or let a
payload rule decide membership, and a group's **first element changes** — so
its ordinal changes, so every stored record of every affected conversation
looks new on the next poll, **and the conversation silently doubles.**

Of the four readers in this plan, this derivation and metrics-db's are
*group-derived* while omp's and pij's are *record-derived*, so this one carries
strictly more of that risk: a record-derived ordinal can only be broken by
changing the field, while this one can also be broken by changing what counts
as a neighbour.

**The live example, which is not hypothetical.** Dropping `thinking` (below)
discards a block's TEXT but never its line's membership. The first block of a
group is routinely a thinking block — `9ccf07af` in the committed fixture is
one — so a reader that skipped thinking *lines* instead of their *content*
would move that group's ordinal to `82ab2abe` and double every claude
conversation already stored. `dropping_thinking_does_not_move_an_ordinal`
exists to make that failure loud.

---

## The dialect

Framing — the tail buffer, the byte-offset cursor, rotation and truncation
detection, the torn-line rule, and refusing a foreign cursor — is
`conversation_sources::tail` and is not reimplemented here. What follows is
what makes this store different from the other three.

### One line per content BLOCK

Claude writes a separate jsonl record for every content block a message
contains, each repeating the same `message.id`. On the committed fixtures:
session `a5a5588f` holds **38 `assistant` records over 13 distinct
`message.id` values**; `b1d6f4fb` holds 6 over 2. A reader that emits one turn
per record reports 38 assistant turns where a human reading the transcript sees
13.

**The blocks of one message are not adjacent in the file.** A tool-use loop
writes `assistant`(tool_use) → `user`(tool_result) → `assistant`(same
`message.id`). Collapsing *adjacent runs* yields **20** groups on that fixture
instead of 13 — a bug that passes review, passes a count test that was written
against the wrong number, and is only visible to someone reading a transcript
who notices an assistant message split around its own tool call.

So grouping is **keyed by `message.id` over the assistant projection**, and the
merged turn is emitted at the position of its **first** block, which keeps a
batch in store order. What makes one pass sufficient: no `message.id` ever
reappears after a different one has intervened — the projection is grouped, and
only the interleaved `user` records break adjacency in the raw file.

`a_message_interrupted_by_its_own_tool_result_stays_one_turn` is the guard, and
it is mutation-checked: it fails if the merge is ever changed to an adjacent-run
fold. (Verified by doing exactly that — see Proof below.)

### A group split across two polls yields two turns, permanently

A live session can be polled mid-message. This reader does **not** hold back the
trailing group, because a session that *ends* mid-message would then never emit
its final turn at all — silent loss, on exactly the conversation someone is
watching live.

The consequence is permanent, not a delay, and that is the part a reader of this
page needs:

1. Poll N stores blocks 1-2 under `uuid(b1)`.
2. Poll N+1 stores block 3 under `uuid(b3)`.
3. A later rotation forces a rescan, which regroups all three blocks and emits
   ONE record under `uuid(b1)` — which the dedupe ledger has already seen, so it
   is dropped.
4. **The turn stored under `uuid(b1)` therefore keeps blocks 1-2 forever and is
   never backfilled.**

Nothing is lost and nothing duplicates: one assistant message simply reads as
two turns. Accepted for v1 by PM ruling (plan 005, wave 1).

### A tool result's NAME can fall back to its id, permanently

A `tool_result` record names only the `tool_use_id` of the call it answers.
The tool's name — `Bash`, `Write` — lives on the `tool_use` block that made the
call, and `toolUseResult` never carries it (checked across every committed
fixture). So the name is recovered from the `tool_use` seen **in the same
batch**, and where that is not available the `tool_use_id` is used as the tool
name instead.

In practice a call and its result are adjacent, so the lookup almost always
hits. It misses when a poll lands exactly between them — the call was consumed
by the previous batch and is no longer in view. Carrying the map across polls
would need reader-side state that the frozen contract has nowhere to put.

**When that happens the stored turn keeps the id as its tool name FOREVER.**
A later rescan would re-read the call and the result together and resolve the
real name — but the dedupe ledger has already seen that ordinal, so the rescan
is dropped and the stored turn is never revisited. This is the same permanence
shape as the split-group case above, and for the same reason: **a rescan
repairs nothing, because dedupe is what a rescan is for.**

It affects the `tool` field of a `ToolResult` item only. The ordinal, the
dedupe and the turn's prose are unaffected.

### The record-type allowlist is a BEHAVIOUR, not an enumeration

The committed fixtures alone hold **14** distinct record types. Only `user` and
`assistant` bear turns. Everything else is store bookkeeping that describes the
session rather than anything said in it, and is dropped:

| dropped type | what it is |
| --- | --- |
| `attachment` | file payload attached to a prompt; body is not conversation |
| `last-prompt`, `custom-title`, `ai-title` | session labelling, rewritten in place |
| `agent-name`, `mode`, `permission-mode`, `atis-latch` | session settings at a point in time |
| `pr-link` | a PR association, not an utterance |
| `file-history-delta`, `file-history-snapshot` | editor undo state |
| `queue-operation` | prompt-queue bookkeeping |

**The rule is stated as a behaviour on purpose: anything not turn-bearing is
dropped, including a type this reader has never heard of. An unknown record type
is a DROP, never an error and never a panic.** Anthropic will add a 15th type,
and an ingest that dies because the store grew a bookkeeping row is a worse
outcome than one that ignores it. A malformed or unparseable line is skipped for
the same reason.

This rule exists because the packet enumerated 13 types from one session of two
and omitted `ai-title`; the fix was never a longer list, it was refusing to make
the list load-bearing. `an_unknown_record_type_is_dropped_and_its_neighbours_still_parse`
is the guard.

### Spilled tool results

An oversized tool result is written to `<session>/tool-results/<name>` and the
record keeps only a ~2KB preview in `toolUseResult.stdout`.

The record also carries `toolUseResult.persistedOutputPath` — **an absolute path
belonging to the machine that wrote it** (`/Users/agent/.claude/projects/...` in
the committed fixture), which does not exist on the machine reading it. Only its
**file name** is portable, so the spill is resolved as
`<session dir>/tool-results/<file name>` and the absolute path is never opened.

A resolved spill contributes its **full bytes** with `truncated: false`. A spill
that cannot be read falls back to the preview with `truncated: true` and the
store's own `persistedOutputSize` as `total_bytes` — a tool result that cannot
be read is a smaller result, not a failed ingest.

### A sidecar's parent comes from the DIRECTORY

A subagent sidecar is its own conversation (`SessionKind::Subagent`), never
folded into the parent's sequence. Its `.meta.json` carries `agentType`,
`description`, `toolUseId` and `spawnDepth` — **but no parent session id.** The
directory it sits in is the only place that link exists, which is not where the
next person will look for it.

`resolve()` re-globs `subagents/` on **every** call and never caches: a subagent
spawned after ingestion began is a child conversation that starts at offset
zero, and a reader that resolves once loses it.

---

## This reader is LOSSLESS; payload policy belongs to the normaliser

The v1 payload policy — head-truncating tool results to 512 B, eliding
write-family tool bodies to a path plus a length — is the **normaliser's** job.
That is settled by the frozen contract itself, not merely by convention:
`fs3_core::conversation_source`'s rustdoc, explaining why records are already
semantic, ends "what is left for the normaliser is genuinely pure: assign the
ordinal, apply the payload policy, drop what v1 does not store."

So everything here is verbatim: tool inputs are whole and a resolved spill is
its full bytes. `ToolInput::Elided` is minted by the normaliser and never by
this reader, and the 512-byte cut happens once, downstream. A reader that
applied the policy itself would destroy data the policy might later want back,
and a policy applied in two crates is a policy that will drift.

### `thinking` blocks are DROPPED at the reader

**Ruled by prime, 2026-08-28 (plan 005, option A). `thinking` blocks never
reach `RawRecord::body`.** The omp reader does the same, so one harness cannot
index model reasoning while another does not.

**Why it must happen here and cannot happen downstream:** a block's type
survives only until the blocks are concatenated into one body string. After
that, no normaliser can tell which prose was thinking — and neither the
normaliser nor the core conversation types have any concept of a thinking block
to work with. A rule that is only implementable at the reader must be applied
at the reader. This is the load-bearing reason.

**What the justification is NOT.** Two different cost-shaped rationales have
been attached to this rule and both are false for this store; they are recorded
here so neither is reintroduced:

1. The payload spec says thinking is "(absent in claude data anyway; cannot be
   a contract)". False — the committed fixtures hold **21 thinking blocks**
   against 5 `text` blocks.
2. It was then argued that this 4:1 dominance would make the claude index
   mostly model reasoning and multiply the embed bill. Also false, and the
   correction is mine: that is a count of BLOCKS, not of bytes.

**Measured: claude does not persist thinking TEXT at all.** Every one of those
21 blocks carries an encrypted `signature` of 452-2068 bytes and a `thinking`
field of length **zero** — 0 bytes of reasoning prose across the entire claude
fixture set. The harvest did not do this: its provenance records 0 credential
redactions, and its body cap leaves a visible `…[fixture-truncated]` suffix
rather than an empty string. The store writes the block and withholds the text.

So for claude the drop removes 21 empty blocks and saves no index bytes and no
embed spend. It is done for **structural correctness and cross-harness
consistency**, not for cost — and because a reader that concatenated thinking
would silently begin indexing model scratch the day Anthropic starts persisting
it, with no code change to notice.

`claude_does_not_persist_thinking_text` pins the 21-blocks/0-bytes measurement
so this cannot rot; if it ever fails, claude has begun persisting reasoning and
the v1.1 question of storing thinking distinguishably (deferred by name to the
live-capture plan) should be reopened. The drop rule itself is proved by
`a_thinking_block_never_reaches_a_turn_body`, which must be driven from a
**synthetic** session — the committed fixtures contain no reasoning for a
broken reader to leak, so they cannot prove that rule.

**The drop discards a block's TEXT, never its line's group MEMBERSHIP** — see
the frozen grouping rule below, which this would otherwise break.

### Roles and sources

| record | role | source |
| --- | --- | --- |
| `assistant` | `Agent` | `System` |
| `user`, `origin.kind = "human"` | `Human` | `Human` |
| `user`, `origin.kind` anything else | `Human` | `Peer` |
| `user` carrying tool results | `Human` | `System` |
| `user` with `isMeta` or `isCompactSummary` | `Human` | `System` |
| `user`, plain prose | `Human` | `Human` |

Claude's store has **no signal** distinguishing a peer-injected turn from a
typed one — a `pij send` and a human message are byte-identical here — so
`TurnSource::Peer` is reported only when `origin.kind` says so. Guessing from
message text would be dialect invention rather than dialect reading.

`head_sha` is always `None`: the records carry `gitBranch`, and a branch name is
not the thing `head_sha` promises.

---

## Snap-in recipe

The exact wiring for the composition root. This unit does not wire itself in.

**1. The module line** — already present, kept alphabetical, in
`crates/providers/src/conversation_sources/mod.rs`:

```rust
pub mod claude;
```

**2. Construction.** The reader takes the workspace-slugged project directory
that holds the session files. It never derives the slug itself — resolving a
workspace folder to a slug is the orchestrator's job, and keeping it out of the
reader is what lets tests point it at a directory that is not under `$HOME`.

```rust
use fs3_providers::conversation_sources::claude::ClaudeSource;

let source = ClaudeSource::new(claude_projects_dir); // ~/.claude/projects/<slug>
```

**3. Registration**, as a trait object alongside the other readers:

```rust
let sources: Vec<Box<dyn ConversationSource>> = vec![
    Box::new(ClaudeSource::new(claude_projects_dir)),
    // ... the other three readers
];
```

**4. Config shape.** One value is needed, and it has a derivable default:

| key | type | default | meaning |
| --- | --- | --- | --- |
| `claude.projects_dir` | path | `$HOME/.claude/projects/<slug>` | the workspace-slugged directory holding `<session>.jsonl` |

The slug convention for this store is the `-Users-...` path-mangling form.
(Measured, and noted because it does **not** generalise: the omp store strips
the home prefix instead, so resolution genuinely differs per store.)

**5. Calling it.** `ConversationSource` is blocking by design — every operation
is file IO — so the composition root hands it to `spawn_blocking`, exactly as it
does the local ONNX embedder.

`resolve()` accepts only `IngestInput::Native { harness: Harness::Claude, .. }`.
An `IngestInput::Pij` seat is **refused**, not joined: the seat-to-session
lookup is the orchestrator's, and doing it here would put a pij dependency
inside a claude dialect.

---

## Proof

`cargo test -p fs3-providers --test conversation_source_claude` — **19 tests**.

- `the_claude_reader_satisfies_the_conversation_source_contract` — the shared
  five-case suite (`fs3_testkit::conversation_source_contract`), driven through
  a `SourceFixture` over a **scratch copy** of the fixtures in a temp dir. The
  committed bytes are never written to; the growth case appends real records
  taken from the other committed session, so the fixture grows by something the
  store would actually write.
- `emitted_ordinals_are_a_subsequence_of_what_the_store_holds` and
  `the_committed_fixtures_are_unchanged` — the committed expectations.
- `assistant_blocks_merge_by_message_id_to_the_count_the_fixture_pins` — asserts
  against `extras.distinct_assistant_message_ids` in the expectations file
  rather than a literal, so the fixture pins the arithmetic.
- `a_message_interrupted_by_its_own_tool_result_stays_one_turn` and
  `an_adjacent_run_fold_cannot_pass_this` — the shape guards.
- `dropping_thinking_does_not_move_an_ordinal` — the frozen grouping rule.
- `a_thinking_block_never_reaches_a_turn_body` (synthetic) and
  `claude_does_not_persist_thinking_text` (measured) — the drop rule and the
  measurement that corrects its justification.

### The structural done-bar does not catch the bug this unit most feared

**The mutation check was verified by performing the mutation**, not asserted.
Changing the merge to an adjacent-run fold (closing open groups at each `user`
record) turns 13 assistant turns into 20 and fails four tests.

But `emitted_ordinals_are_a_subsequence_of_what_the_store_holds` **still
passed** under the broken implementation — a subsequence of 20 ordinals is
still a valid, in-order, repeat-free subsequence. So the committed structural
expectation, which is the plan's mechanical done-bar for a reader, **cannot
distinguish a correct merge from a broken one.**

That is why `an_adjacent_run_fold_cannot_pass_this` exists and why it pins the
SHAPE — the count of distinct assistant ordinals, and the specific continuation
blocks that must never become ordinals of their own — rather than a total.
A count-only test written against the wrong number would have passed too.
