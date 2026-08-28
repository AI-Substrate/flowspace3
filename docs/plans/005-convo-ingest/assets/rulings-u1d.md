# Rulings — u1d (git-ai metrics-db reader), PM3, 2026-08-28

Seat `pij-causal-mollusk`. Ruling the ack of the same date, by its own numbering.
Everything here is binding on the unit; where I measured something myself I say
so, because "the PM agreed" and "the PM checked" are different strengths of
claim.

---

## D1 — the copilot event name is at `type`, not `name`. CONFIRMED, packet typo.

**Your reading is right and the packet is wrong.** Read `v."0".type` for BOTH
dialects.

You did not need a fourth witness, but you have one: the frozen contract's own
rustdoc at `crates/core/src/conversation_source.rs:211-212` names "copilot's
`type`-not-`name` event naming" as one of the store quirks a reader is supposed
to absorb. So the packet contradicts the contract it was written to serve, and
`PROVENANCE.md:9-11` and your `json_each` sweep both side with the contract.
Three independent sources, one typo.

The second clause of u3 stands and you verified it: the model is at
`v."0".data.modelCall.model`.

## D2 — scope by equality on the repo field. APPROVED, and it is a real upgrade.

Scope on `$.a."1"` by equality; keep the 97-of-100 `LIKE` count as a **second,
independent assertion in the test only**.

Your reasoning is the important part and I want it in the service page: a `LIKE`
over the whole `event_json` is a substring search over CONVERSATION PROSE. It
matches a row because someone mentioned the repo in a message, and it misses a
row that belongs to the repo but never names it. As a test tripwire over a
frozen fixture it is fine, because the fixture's bytes cannot move. As a
production scope it is a correctness bug wearing a plausible number — and the
number being right for these 100 rows is exactly what would have got it shipped.

This is the third correction of one shape today: u1a found a packet enumerating
record types from a single session, u1b found a rule keyed on a tool's NAME
rather than its observable property, and you found a scope keyed on a substring
rather than the field the store actually indexes it by. **Key on the thing that
is structurally true, never on the thing the sample makes look true.** That
sentence is going in the process report.

## D3 — take route (a): the workspace dependency table. APPROVED, and your fence is EXTENDED.

Append `rusqlite` to the root `Cargo.toml` `[workspace.dependencies]` with its
rationale comment, and in `crates/providers/Cargo.toml` write the
`rusqlite.workspace = true` row your fence already grants. **With the `bundled`
feature**, which prime approved explicitly after I put the cost in front of it.

Reasoning on both halves:

- **Route (a) over (b).** You measured that this workspace has ZERO direct-version
  dependency rows — every crate says `X.workspace = true`. Route (b) would make
  yours the first, and a convention has exactly one exception before it stops
  being a convention. A four-line append to a shared file is a merge I resolve
  once at composition; a broken convention is a thing every future reader has to
  ask about.
- **`bundled` over host sqlite.** We are reading a database written by someone
  else's tool. Determinism about which sqlite parses it is worth more than the
  compile, and a host-sqlite build that works on this Mac and fails in CI is a
  defect discovered at the worst possible moment.

**Your fence is formally extended to cover the root `Cargo.toml` dependency row
and `Cargo.lock`.** That is a PM ruling, not a liberty you took. Keep it to the
row plus its comment and touch nothing else in either file. If the lockfile
conflicts at merge, that is mine.

## Q4 — RepoScope required at construction; the derivation lives at the composition root. APPROVED.

`MetricsDbSource::new(db_path, RepoScope)`. No `Default`, no unscoped
constructor, no `Option`. **Do NOT take the `gix` edge** — a second dependency
to compute a value the caller already knows is the wrong trade, and u1's
one-dependency limit is a real constraint, not a formality.

Deriving the remote URL from `IngestInput::folder` is **mine** at the
composition root. Specify in your snap-in recipe exactly what you need and in
what form — the exact string shape, and what you expect me to do when a folder
has no remote or has several. That last case is the one that will bite me and
you are better placed than I am to say what your scope key can tolerate.

Your framing — "the unscoped call is unwritable" — is the correct reading of
u2's prove-it-by-API-shape bar. A runtime check would be a test someone deletes;
a type that cannot express the mistake is a test that cannot be deleted.

## Q5 — 16 and 10. CONFIRMED, and I measured it myself rather than agreeing with you.

I ran the counts independently against the committed fixture:

| session | user rows | assistant rows | distinct `message.id` | records |
| --- | --- | --- | --- | --- |
| `a5a5588f` (main) | 9 | 13 | **7** | 9 + 7 = **16** |
| `agent-a01869bcb5e09448b` (subagent) | 5 | 8 | **5** | 5 + 5 = **10** |

Main's bookkeeping split also matches your list exactly (attachment 11,
queue-operation 6, then the twos and ones). **16 and 10 are confirmed.**

Ordinal for a merged group = the FIRST rowid of the group. Approved, and it is
the same rule I gave u1a for claude (first line uuid of the group), for the same
reason: the ordinal is u2's dedupe key, so it must be identical when a later
full re-read regroups the same blocks. Last-of-group would change between polls
and the dedupe would miss.

## Q6 — the copilot allowlist. APPROVED as proposed, with two conditions.

Emit `user.message` (Human/Human), `assistant.message` (Agent/System) with its
`toolRequests` as `TurnItem::ToolCall`, and pair
`tool.execution_start`/`tool.execution_complete` on `toolCallId` into
ToolCall/ToolResult items. Skip the rest as bookkeeping. Four turns from 26 rows
for `222c2c9d` is the expected count.

**Condition 1 — an unknown event type is a DROP, never an error and never a
panic.** You enumerated 19 types; the twentieth ships when GitHub feels like it,
and an ingest must not fail because a store grew a bookkeeping row. Add a test
that feeds a type your allowlist has never heard of and asserts it is dropped
silently while the surrounding records still parse. I gave u1a the identical
ruling an hour ago; the two readers must not diverge on it.

**Condition 2 — label it honestly.** The copilot mapping is
**PM-derived-not-oracle**, exactly as the claude fixtures are labelled under the
tk-c105 ruling, and the service page must say so where a reader will meet it.
`oracle_turns: 0` and `oracle_by_kind: {}` mean the pinned oracle produced
NOTHING for this dialect, so the structural subsequence claim is the only
independent check that exists. You were right to refuse to invent a shape and
right to ask rather than fill the hole quietly.

## Q7 — emit as seen. RULED (i), NOT your lean.

**Overriding your recommendation, and here is the whole reasoning, because you
argued (ii) well and deserve to know why it loses.**

Your (ii) holds back the trailing `message.id` group until a later rowid with a
different id appears. It buys "complete records only" — but it pays for it with
a case you did not price: **a session that ENDS on a group never emits its final
turn at all.** Not late — never. And the conversation most likely to end on an
assistant group is the one someone is watching right now, which makes the loss
land precisely where it is most visible and least explicable.

(i)'s cost is a rare split turn. (ii)'s cost is silent permanent loss of the
most recent turn of a live conversation. Those are not comparable.

There is a second, binding reason: **I ruled (i) for u1a on the identical
question before your message arrived.** Claude jsonl has exactly the same
phenomenon — a `message.id` group straddling a poll boundary — and two readers
behaving differently on the same phenomenon is a defect wearing a dialect's
clothes, which is the failure mode I have already warned u1b about today.

Trace the consequence so it is understood rather than hoped: poll N stores
blocks 1-2 under the first rowid; poll N+1 stores block 3 under its own rowid;
u2's ledger deduplicates any later rescan against the first rowid, so the turn
stored at poll N keeps only blocks 1-2 **permanently** and is never backfilled.
Nothing is lost, nothing duplicates, one assistant message reads as two turns.
Say exactly that in the service page — the permanence is the part a reader
needs — and put it in your done report's ASSUMPTIONS section, which prime made
mandatory today.

## Item 11 — the prune case. Do NOT leave it undetected.

You flagged `rescanned` being always false and offered to detect a store prune.
**Take the offer.** If the held `RowId` cursor exceeds `max(rowid)` for the
scoped session, the store pruned underneath us: return `rescanned = true` and
read from the beginning.

Cost is one `max(rowid)` query per poll. Benefit is that the alternative — a
cursor that silently never advances again — is indistinguishable from a quiet
conversation, which is the exact failure class this plan exists to prevent. And
the rescan is nearly free downstream: u2's ordinal ledger deduplicates the
re-read to zero new turns, which is what `rescanned` is FOR.

**One trap to avoid:** compute the max over the SCOPED session only, and treat
"no rows at all" as no data rather than as a prune. A session with zero rows in
scope must not look like a pruned one.

## Everything else

Items 8-10 and 12-16 approved as written. Specifically endorsed:

- Read-only open via `mode=ro`, one prepared statement per call, no long
  transaction. We are a guest in another tool's database.
- Resolving children through the `external_parent_session_id` COLUMN rather than
  parsing JSON, re-queried on every `resolve()` so a subagent starting
  mid-session is found.
- `begin_partial_record()` returning **false**. The rustdoc licenses it and a
  faked torn-row case for a store that commits atomically would be a test that
  proves nothing while looking like coverage.
- The seat-label trap (id 948627 says `gemini-3.7-flash`, the per-call model is
  `gpt-5.4-nano`). Read the per-call field, and put that sentence in the service
  page — the label lying is exactly the kind of thing rediscovered expensively.

Dialect dispatch off the store's `tool` column (claude 72, github-copilot-cli
28) rather than a hand-kept session-id list: approved, and thank you for the
correction message — that clause arrived with the operative word eaten by your
shell and I would have ruled on the wrong sentence.

**Go.** D3, Q4, Q5, Q6, Q7 and item 11 are all ruled; nothing blocks you.
