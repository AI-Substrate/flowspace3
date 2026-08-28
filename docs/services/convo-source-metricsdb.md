# Conversation source — git-ai metrics-db

**Unit**: plan 005-convo-ingest, u1d · **Module**:
`crates/providers/src/conversation_sources/metrics_db.rs` · **Port**:
`fs3_core::ConversationSource`

Reads agent conversations out of git-ai's machine-wide sqlite metrics store
(`event_kind = 5`). It is the only one of the four readers that faces a database
rather than a file, the only one that sees more than one repository, and the
only place a GitHub Copilot session exists at all.

---

## Read this before you change anything

### The ordinal derivation is FROZEN

> **The ordinal is the decimal string form of the row's `id` column. For rows
> merged by `message.id`, it is the FIRST `id` of the group.**

This is not an implementation detail. The ordinal is the key the durable cursor
ledger deduplicates on, and that key is written to Postgres, where it outlives
every process.

Change how it is derived — a different field, a different rendering,
first-of-group becoming last-of-group, the integer becoming something prettier —
and **every record already stored looks brand new on the next poll. The
conversation silently doubles.** There is no clean recovery: forgetting the
session and re-reading from zero duplicates it again.

First-of-group specifically is what makes the key stable across a re-read that
regroups the same blocks. Last-of-group would change between polls and the
dedupe would miss.

### The grouping rule is frozen too, and it is the sharper edge

This reader's ordinal is **group-derived**, so it depends on a datum *and* on
the rule that decides group membership. Two of the four readers are like this;
the other two key on a single record and carry strictly less risk. The frozen
rule, in full:

- **The row-selection predicate**: only rows matching `event_kind = 5` **and**
  the repo scope exist to be grouped at all. The scope is applied *per row*, so
  it is part of the grouping rule, not a filter that happens before one.
- Of the rows that survive, only `tool = 'claude'` records of type `user` or
  `assistant` are emitted. Every other record type is dropped.
- Of those, rows carrying `message.id` merge into **one** record per distinct
  `message.id`. Rows without one — every `user` row — never merge.
- The record's ordinal is the smallest `id` in its group.

All four bullets are the frozen rule. Widen the emit allowlist, let a new type
join a merge, start including a row that is skipped today, **or change
`event_kind` or the scope expression**, and the **first element of an existing
group can change even though the datum did not**. Every stored record then looks
new and the conversation doubles — the same silent failure as changing the
derivation, reached by touching something that does not look like the derivation
at all.

The predicate bullet is the one that surprises people, so here is the measured
state of the risk it guards.

**Can one session's rows differ in scope?** Not in the committed fixture: all
six sessions carry exactly one distinct `$.a."1"` value each, with no NULL and
no empty string in 100 rows.

```sql
select external_session_id,
       count(distinct json_extract(event_json,'$.a."1"')) as distinct_repos
from metrics group by external_session_id;   -- every row: 1
```

That is a measurement of a 100-row sample, **not a guarantee**. `$.a."1"` is
stamped per event by git-ai, not per session, so nothing structural stops a
long-lived session from spanning a `git remote set-url` or a working directory
change. And the fixture cannot speak to the `event_kind` half at all: the
harvest selected `event_kind = 5`, so all 100 rows carry it by construction.

So treat the freeze as load-bearing rather than hygiene. If scope ever does vary
within a session, a change to the scope expression moves which row opens a
group, and the failure is silent.

### The `id` column survives a VACUUM — here is why that is safe

A sqlite `rowid` is normally **not** stable: `VACUUM` renumbers rows. That would
be fatal here, because a renumbered store breaks every persisted ordinal at
once, in a database we do not control. It does not apply, and this is the
evidence rather than the reassurance — git-ai's own DDL:

```sql
CREATE TABLE metrics ( id INTEGER PRIMARY KEY AUTOINCREMENT, event_json TEXT NOT NULL, ... )
```

`INTEGER PRIMARY KEY` makes `id` an **alias** for the rowid, and sqlite only
renumbers tables that lack one, so `VACUUM` cannot move these values.
`AUTOINCREMENT` additionally guarantees an id is never reused after a deletion.

The queries name `id` explicitly rather than the bare `rowid` keyword. They are
the same value here; naming the aliased column is what makes the stability
visible to whoever reads the query next.

### A message that straddles a poll boundary is stored as two turns, permanently

The store writes one row per content block, so one assistant message can arrive
as four rows sharing a `message.id`. This reader merges them — but it **emits
what it can see** rather than holding a group back to check whether more blocks
are coming.

The consequence, traced, because the permanence is the part that matters:

1. Poll N sees blocks 1–2 and stores a turn under the first `id`.
2. Poll N+1 sees block 3 and stores it under its own `id`.
3. The ledger deduplicates any later rescan against the first `id`, so the turn
   stored at poll N **keeps only blocks 1–2 forever** and is never backfilled.

Nothing is lost, nothing duplicates, and one assistant message reads as two
turns.

**A rescan does not heal it, and that is worth knowing before you try one.** A
prune-triggered rescan regroups from zero and re-emits the whole message as a
single record under its *original* first `id` — which the ledger has already
seen, so it is dropped as a duplicate. The split survives the one operation that
looks like it should repair it. This is correct behaviour, not a gap: the
alternative is a rescan that rewrites stored turns, which is how a dedupe key
stops being a dedupe key.

If you are looking at two turns where the store holds one message, this is why.
It is by design.

The design was ruled deliberately (PM, 2026-08-28) against the alternative of
holding the trailing group back until a later `message.id` appears. That
alternative buys "complete records only" and pays with a worse failure: **a
session that ENDS on a group never emits its final turn at all** — not late,
never — and the conversation most likely to end on an assistant group is the one
someone is watching live. The claude-native reader was ruled the same way on the
identical phenomenon, so the two readers do not diverge.

### The copilot mapping is PM-derived, not oracle-backed

The pinned reference oracle (`reconvo.py`) produced **zero** turns for the
copilot dialect — `oracle_turns: 0`, `oracle_by_kind: {}` in
`expectations.json`. Nothing independent pins which copilot events become turns.

The only external check this half of the reader gets is the structural claim
that its ordinals are an in-order, repeat-free subsequence of the ids the store
holds — which catches an invented, reordered or duplicated record, and **cannot
catch a wrong allowlist**. The allowlist below was proposed by this unit and
ruled by the plan's PM, and is labelled here exactly as the claude fixtures are
labelled under the tk-c105 ruling.

### The rusqlite version is not free

`libsqlite3-sys` declares `links = "sqlite3"`, and cargo permits exactly one
package with a given `links` value in the graph. `sqlx-sqlite` already puts
`libsqlite3-sys 0.30.1` there, so `rusqlite` must be the release that shares
it — **0.32**.

Pick another and the resolver walks `sqlx` backwards looking for an escape,
then fails on a missing `tls-rustls-ring` feature, which reads as an unrelated
TLS problem and sends you the wrong way entirely. If you bump `sqlx` and the
`rusqlite` row starts failing, those two move together.

---

## What the store looks like

| fact | value |
| --- | --- |
| table | `metrics`, filtered to `event_kind = 5` |
| record | `event_json`, with the native record at `$.v."0"` |
| dialects | `tool = 'claude'` (mirror) and `tool = 'github-copilot-cli'` |
| repo | `$.a."1"`, a remote URL, on **every** row of both dialects |
| session | `external_session_id` column |
| parent | `external_parent_session_id` column |
| cursor | `rowid`; `id` is `INTEGER PRIMARY KEY`, so `id == rowid` |
| timestamp | `$.v."0".timestamp`, ISO-8601 UTC; `event_ts` column as fallback |

`event_ts` is second-grain and **not** unique — 17 timestamps in the committed
fixture carry more than one row — so it collides precisely when a conversation
is busiest. Only `rowid` is a safe cursor.

### Both dialects name the event at `type`

The plan packet said copilot carries its event name at `$.v."0".name`. **It does
not.** No row in the store has that path, and copilot's `$.v."0"` key set is
exactly `{data, id, parentId, timestamp, type}`. The frozen contract's own
rustdoc names "copilot's `type`-not-`name` event naming", and the fixture's
`PROVENANCE.md` agrees. Confirmed a packet typo by the PM, 2026-08-28.

### The seat label lies about the model

Row 948627 (`session.shutdown`) reports `data.currentModel` =
`gemini-3.7-flash`, while the actual per-call model on rows 936664–936666 is
`gpt-5.4-nano` at `$.v."0".data.modelCall.model`. **Read the per-call field.**
The session-level label is a display string, not a fact, and rediscovering that
is expensive.

### What becomes a turn

Both dialects use an **emit allowlist**, never a skip list. The store's
bookkeeping vocabulary grows without telling us, and a skip list silently
promotes every new bookkeeping type to a conversation turn. An unrecognised
event type — or an unrecognised `tool` — is **dropped**, never an error: an
ingest must not fail because the store grew a row.

**claude mirror** — emit `user` and `assistant`; merge by `message.id`; drop the
twelve bookkeeping types (`attachment`, `queue-operation`, `mode`,
`permission-mode`, `last-prompt`, `custom-title`, `agent-name`, `atis-latch`,
`pr-link`, `system`, `file-history-delta`, `file-history-snapshot`).

`thinking` blocks are **not** prose and do not reach `body`. The reference
oracle does not render them either, which makes agreement with it definitional
rather than lucky — the committed expectation compares an assistant body by
sha256, so a body carrying thinking text would fail a claim that is meant to
pin agreement.

**copilot** — emit `user.message`, `assistant.message` (its `toolRequests`
become `ToolCall` items), and pair `tool.execution_start` with
`tool.execution_complete` on `toolCallId`. Everything else — `turn_start`,
`turn_end`, the eight `model.*` telemetry events, `session.*`, `hook.*` — is
bookkeeping.

### Turn sources

`role` alone would report an orchestrated fleet as half-human (workshop 005,
C8), so three sources are distinguished:

- a compaction summary (`isCompactSummary`) is `System` — written by the harness
  wearing a user turn's clothes, and never dropped, because it is the only
  record of what the discarded context said;
- a body opening `[pij from ` is `Peer` — a packet injected by another agent;
- everything else a human typed is `Human`.

### `head_sha` is always `None`

This store records a repo remote and a git branch, never a HEAD sha. Claiming
one would be an invention. The orchestrator supplies it if it wants one.

---

## Operating notes

**Read-only, always.** The live store was 4.2 GB with a 47 MB uncheckpointed WAL
at fixture-harvest time. This reader opens `file:...?mode=ro` with
`SQLITE_OPEN_READ_ONLY`, holds one prepared statement per call and no
transaction across calls. It must never write, never checkpoint and never hold a
long read against a database another tool is actively appending to.

**A read-only open of a WAL database needs a readable `-wal` and a writable
directory** for the `-shm` file. That is a property of sqlite, not of this
reader. If a deployment runs as a user who cannot write the store's directory,
the open fails and says so rather than silently reading stale data.

**The store self-prunes.** `schema_metadata` carries a `metrics_last_prune_ts`
watermark. When a held cursor exceeds `max(rowid)` for the scoped session, this
reader reports `rescanned = true` and re-reads from zero; the ordinal ledger
deduplicates it back to nothing. A session with **no rows in scope** is not a
prune — it is an empty session, and treating it as one would make every empty
poll a full re-read.

---

## Snap-in recipe

Everything the composition root needs. This unit does not wire itself in.

### 1. The module line

Already present in `crates/providers/src/conversation_sources/mod.rs`:

```rust
pub mod metrics_db;
```

### 2. Construction

```rust
use fs3_providers::conversation_sources::metrics_db::{MetricsDbSource, RepoScope};

let source = MetricsDbSource::new(
    metrics_database_path,              // impl Into<PathBuf>
    RepoScope::remote_url(remote_url),  // impl Into<String>
);
```

There is deliberately **no unscoped constructor**: no `Default`, no
`new(path)`, no `Option<RepoScope>`. This store is machine-wide, so an unscoped
read is a data leak rather than a convenience, and a type that cannot express
the mistake outlives a test that merely catches it.

The reader is `Send + Sync` and blocking. Hand it to `spawn_blocking` exactly as
the composition root already does the local ONNX embedder.

### 3. The database path

git-ai's store, `~/.git-ai/internal/metrics-db` on the harvested machine. It is
a fixed per-machine location, not per-repository — that is the whole reason the
scope exists.

### 4. What `RepoScope` needs, exactly

**The remote URL as the writing tool recorded it**, compared by exact string
equality against `$.a."1"`. On the harvested machine that is
`https://github.com/AI-Substrate/flowspace3` — scheme and host included, no
`.git` suffix, no trailing slash, not a path, not a slug, and not a normalised
form of any of those.

Deriving it from `IngestInput::folder` is the composition root's job. This
reader deliberately does not take a git dependency to compute a value the caller
already knows.

Two cases to decide there, both of which this reader cannot see:

- **A folder with no remote.** There is no scope key, so there is no safe read:
  every row in the store belongs to some other repository. Fail the ingest with
  a message naming the folder. Do **not** fall back to an unscoped read, and do
  not invent a scope from the directory name — the store keys on the remote and
  nothing else.
- **A folder with several remotes.** Pick deliberately and record which:
  `origin` is the conventional answer, and a fork's `upstream` is a different
  repository as far as this store is concerned. If two remotes could both be
  right, that is an operator question, not a default — surface it rather than
  guessing, because guessing wrong silently indexes the wrong project's
  conversations and the failure looks like missing data.

An empty scope string is not a wildcard. It matches nothing, which is the safe
direction.

### 5. Resolution and reading

```rust
let files = source.resolve(&IngestInput::Native {
    session_id,               // the external_session_id
    harness: Harness::MetricsDb,
    folder,
})?;
```

`resolve` returns one `SessionFile` **per session**, not per file: `path` is the
database and `session_id` is the `external_session_id`, which is what keeps the
cursor per-conversation. Exactly one is `SessionKind::Main`; subagents are
`SessionKind::Subagent` and name their parent.

Call it on **every** poll. A subagent that starts mid-session is a child
conversation, and a reader that resolves once loses it.

A session that is not in scope is an **error**, not an empty result. Invisible,
not merely unread.

```rust
let batch = source.read_incremental(&file, cursor.as_ref())?;
```

The cursor is `SourceCursor::RowId` only. A `ByteOffset` or `Seq` cursor from
another store is **refused** — read as zero it would silently re-ingest an
entire conversation, and the caller would see a burst of duplicates with no
error to explain them.

### 6. Configuration shape

The reader itself needs no config beyond its two constructor arguments. If the
daemon grows a config block for this store, the two fields are the database path
(defaulting to git-ai's fixed location) and nothing else — the scope is
per-ingest, derived from the folder being ingested, and must not become a
configured global.

---

## Proof

`crates/providers/tests/conversation_source_metrics_db.rs`, 16 tests, offline,
over a scratch copy in a temp directory. The committed fixture bytes are pinned
by sha256 and asserted unchanged on every run.

| claim | test |
| --- | --- |
| the shared contract | `the_reader_satisfies_the_shared_contract` |
| fixtures unchanged | `the_committed_fixtures_are_unchanged` |
| ordinals are a subsequence | `emitted_ordinals_are_a_subsequence_of_what_the_store_holds` |
| oracle prose, verbatim and in order | `the_oracle_prose_appears_verbatim_and_in_order` |
| merge arithmetic (16 and 10) | `the_two_sessions_yield_the_records_the_merge_arithmetic_predicts` |
| compaction kept, marked `System` | `a_compaction_summary_is_kept_and_marked_as_written_by_the_harness` |
| injected packet is `Peer` | `an_injected_peer_packet_is_not_reported_as_a_human_turn` |
| scoping by exclusion | `a_foreign_repo_session_is_invisible_to_a_scoped_reader` |
| scoping cross-check, 97 of 100 | `the_fixtures_own_ninety_seven_of_one_hundred_claim_still_holds` |
| scoping by API shape | `the_unscoped_read_has_no_spelling` |
| copilot dialect off the `tool` column | `the_copilot_dialect_is_read_from_the_stores_own_tool_column` |
| copilot call/result pairing | `a_copilot_tool_call_and_its_result_land_on_one_turn` |
| unknown event type is dropped | `an_event_type_this_reader_has_never_heard_of_is_dropped_not_fatal` |
| prune reported as rescan | `a_pruned_store_is_reported_as_a_rescan_rather_than_going_quiet` |
| empty scope is not a prune | `a_session_with_no_rows_in_scope_is_not_mistaken_for_a_prune` |
| subagent names its parent | `the_subagent_names_its_parent_so_its_work_is_not_invisible` |

The suite was mutation-checked rather than merely observed green:

- **Stop merging `message.id` groups** → 3 tests fail, including the shared
  contract.
- **Drop the repo predicate from the scoped query** →
  `a_foreign_repo_session_is_invisible_to_a_scoped_reader` fails.

Worth knowing what the committed expectations do **not** catch, because it
changes where review effort is worth spending: under the no-merge mutation, both
`assert_ordinals_are_a_subsequence` and `assert_oracle_prose_appears` still
**passed**. The subsequence claim catches an invented, reordered or duplicated
ordinal; it does not notice that 22 records arrived where 16 were correct. The
merge arithmetic is held by this unit's own count test and by the contract's
`expected_records`, and nowhere else.
