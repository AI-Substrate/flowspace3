# Ack — u1d git-ai metrics-db reader

**Seat**: pij-causal-mollusk · **Unit**: u1d · **Branch**: `005-convo-u1d` · **PM**: pij-pale-silkworm
**Status**: HOLDING. Zero lines of implementation code written.
**Written**: 2026-08-28, after the pij-send corruption standing order (DL-006).

This file is the canonical, byte-exact copy of my numbered plan. The version that
reached the PM over `pij send` was corrupted by shell command substitution (one
backtick pair, item 13). Rule against THIS file. Nothing here has passed through
shell word expansion.

Everything below is measured against the committed fixture
`crates/testkit/fixtures/conversations/metrics_db/metrics.sqlite3`, opened
read-only, and against the frozen contract. Every number is reproducible with the
query given beside it.

---

## Part 1 — Defects in my packet

### D1. Packet instruction u3 is wrong about where the copilot event name lives

**Packet u3 says**: copilot "carries it at `v."0".name`".

**Measured**: nothing anywhere in the fixture has that path.

```sql
-- 0
select count(*) from metrics
where json_extract(event_json,'$.v."0".name') is not null;

-- the complete copilot v."0" key set: data, id, parentId, timestamp, type
select distinct key from metrics, json_each(metrics.event_json,'$.v."0"')
where tool='github-copilot-cli' order by 1;
```

The event name is at `v."0".type` — 19 distinct values across the 28 copilot rows,
e.g. `assistant.message` (3), `user.message` (1), `tool.execution_start` (1),
`session.shutdown` (1).

**Two independent sources already agree with the measurement, against the packet**:

- `crates/testkit/fixtures/conversations/metrics_db/PROVENANCE.md:9-11` — "copilot
  event-stream dialect: event name under `v."0".type`".
- The FROZEN contract's own rustdoc, `crates/core/src/conversation_source.rs:211-212`
  — "copilot's `type`-not-`name` event naming".

**u3's second clause is CORRECT** and I verified it independently: the per-call model
is at `v."0".data.modelCall.model` = `gpt-5.4-nano` on ids 936664, 936665, 936666.

**Ask**: confirm I read `v."0".type` for BOTH dialects, and that `name` is a packet
typo rather than a store shape I have failed to find.

### D2. The scoping predicate in u2 is weaker than the store allows

**Packet u2 offers**: `where event_json like '%flowspace3%'` returns 97 of 100.

That count is right, but as a production scope the predicate is a substring search
over the whole record — including conversation **prose**. A row from another repo
whose transcript merely mentions flowspace3 matches; a flowspace3 row that never
spells the word would not, were the field ever absent.

**Measured**: the repository is a first-class field at `$.a."1"`, a remote URL,
present on 100 of 100 rows, in BOTH dialects, with no nulls.

```sql
select ifnull(json_extract(event_json,'$.a."1"'),'<NULL>'), tool, count(*)
from metrics group by 1,2;
```

| repo | tool | rows |
| --- | --- | --- |
| `https://github.com/AI-Substrate/flowspace3` | claude | 71 |
| `https://github.com/AI-Substrate/flowspace3` | github-copilot-cli | 26 |
| `https://github.com/AI-Substrate/pij` | claude | 1 |
| `https://github.com/AI-Substrate/pij` | github-copilot-cli | 2 |

The three foreign rows are exactly ids **943197, 943232, 948060** — the negatives the
packet names.

**Proposal**: scope by equality on `$.a."1"`. Keep the packet's `LIKE` count as a
SECOND, independent assertion in the test suite, so the fixture's own 97/100 claim
stays proven by something other than the code under test.

### D3. Fence gap — I cannot add `rusqlite` inside my fence

My fence grants "crates/providers/Cargo.toml (the rusqlite dependency ROW only)".
That is not sufficient.

**Measured**:

- root `Cargo.toml` `[workspace.dependencies]` has **no** `rusqlite`
- `Cargo.lock` has **no** `rusqlite` (never resolved in this workspace)
- this repo has **zero** direct-version dependency rows in any crate:
  `grep '= { version' crates/*/Cargo.toml` returns nothing. Every crate dep is
  `X.workspace = true`, with the version and its rationale comment in the root table.

So I must either:

- **(a)** append a `rusqlite` row plus rationale comment to root `Cargo.toml`
  `[workspace.dependencies]` — conventional, but a SHARED file outside my fence that
  every seat's merge touches; or
- **(b)** write `rusqlite = { version = "0.32", features = ["bundled"] }` directly in
  `crates/providers/Cargo.toml` — inside my fence, but the workspace's first
  convention break.

**I recommend (a)**, kept to a four-line append. This is a stop-and-ask under packet
rule w2 (dependency additions) and I will not proceed on either until ruled.

**Build risk, flagged**: `bundled` compiles the sqlite C amalgamation on first build.
That is the one genuinely disk- and time-expensive thing in my unit, and it is a
fresh crates.io fetch, since the lockfile has never seen this crate.

The arch-allowlist grant itself is already committed and needs no edit from me —
`crates/testkit/arch-allowlist.toml:97-111`.

---

## Part 2 — Open questions I need ruled before writing code

### Q4. What value carries the repo scope across the API boundary?

`IngestInput` carries `folder: PathBuf` (a workspace path). The store's scope key is
a remote URL (D2). Something must map one to the other.

**Proposal**: the constructor REQUIRES a scope value —
`MetricsDbSource::new(db_path, RepoScope)` — with no `Default`, no unscoped
constructor and no `Option`. The unscoped call is then unwritable, which is the
"prove it by API shape" u2 demands. `RepoScope` is built from an explicit remote URL.

Deriving the remote from `folder` needs git. `gix` is a workspace dependency but NOT
a `fs3-providers` dependency, so taking it would be my SECOND dependency edge, which
u1 makes a stop-and-ask. I therefore propose the derivation lives at the composition
root (u4), and my snap-in recipe specifies it exactly.

**Confirm, or tell me to take the `gix` edge.**

### Q5. Confirm my expected-record arithmetic

Derived from the 12-type bookkeeping allowlist in `PROVENANCE.md:41-42`, plus
merge-by-`message.id`.

**Main session `a5a5588f-0979-439f-a1bf-ddf185a089c7`**: 56 rows

| deduction | count |
| --- | --- |
| bookkeeping skipped | 34 |
| — attachment | 11 |
| — queue-operation | 6 |
| — pr-link, permission-mode, mode, last-prompt, custom-title, atis-latch, agent-name | 2 each = 14 |
| — system, file-history-snapshot, file-history-delta | 1 each = 3 |
| candidates remaining | 22 |
| — `user` rows (no `message.id`, never merged) | 9 |
| — `assistant` rows, folding into 7 `message.id` groups | 13 → 7 |
| **records emitted** | **16** |

**Subagent `agent-a01869bcb5e09448b`**: 15 rows − 2 attachment = 13 = 5 user + 8
assistant in 5 groups → **10 records**.

All 7 shared-id groups `PROVENANCE.md` mentions are in the main session; the subagent
has its own 5, e.g. ids 929034/929035/929036.

The 14 rows in the fixture with no `v."0".timestamp` are ALL bookkeeping types I skip,
so every record I emit has an ISO-8601 timestamp and `RawRecord::at` needs no epoch
fallback on this fixture. I will still fall back to the `event_ts` column for the live
store.

**Ordinal for a merged group = the FIRST rowid in it**, which keeps
`assert_ordinals_are_a_subsequence` in store order.

**Confirm 16 and 10.**

### Q6. Copilot has no oracle at all, and nothing pins its turn set

`expectations.json` gives session `222c2c9d-5798-48cf-9dbd-cd4a52324c53`
`oracle_turns: 0` and `oracle_by_kind: {}`. The pinned `reconvo.py`'s `read_metrics`
produced NOTHING for the copilot dialect. The subset claim therefore bites only on the
two claude sessions, and my copilot mapping is held ONLY by the structural subsequence
claim.

That is a real hole in the done-bar, and I would rather you close it by ruling than
have me invent a shape and call it proven.

**Proposal for the 19 copilot event types:**

- EMIT `user.message` → `TurnRole::Human` / `TurnSource::Human`
- EMIT `assistant.message` → `TurnRole::Agent` / `TurnSource::System`, with its
  `toolRequests` as `TurnItem::ToolCall`
- PAIR `tool.execution_start` with `tool.execution_complete` on `toolCallId` into
  `ToolCall` / `ToolResult` items
- SKIP the rest as bookkeeping: `assistant.turn_start`, `assistant.turn_end`, all
  eight `model.*`, `session.*`, `hook.*`

That yields 4 turns from the 26 rows of `222c2c9d`.

**Rule the allowlist.**

### Q7. The torn-record case, and the sqlite analogue of one

`SourceFixture::begin_partial_record`'s own rustdoc says to return `false` for a store
that cannot be torn — "a sqlite database commits a row or does not have it" — so
contract case 5 is SKIPPED for me by design, not faked. **I will return `false`.**

BUT there is a real metrics-db analogue the contract cannot see. A `message.id` block
group is written as N separate rows, so a live store can be read mid-group (2 of 3
blocks committed). My merge would then emit a partial turn AND advance the cursor past
it — the exact loss the torn-line case exists to prevent. I cannot distinguish "group
finished" from "group still being written" without a lookahead the store does not offer.

**Options:**

- **(i)** emit as seen, accept a rare split turn, document it.
- **(ii)** hold back the trailing `message.id` group of every batch, releasing it only
  once a LATER rowid with a different `message.id` appears. Costs one turn of latency
  per poll.

**I lean (ii)**: it is cheap, it is exactly the tail-buffer discipline the jsonl
readers already get from `tail.rs`, and it makes "complete records only" true rather
than nearly true. It does NOT affect the Q5 counts — both sessions end on a non-group
row.

**Rule (i) or (ii).**

---

## Part 3 — The build, once you have ruled

8. Add the `rusqlite` dependency per the D3 ruling. Verify the arch-allowlist drift
   test passes with NO allowlist edit (the grant is already committed). No second
   dependency.

9. `crates/providers/src/conversation_sources/metrics_db.rs` — public API: `RepoScope`
   (no `Default`, no unscoped path) plus `MetricsDbSource::new(db_path, RepoScope)`.
   Open the database READ-ONLY via `file:...?mode=ro` per u6: no writes, no
   checkpoint, no long transaction. One prepared statement per call, released before
   return. The real store is 4.2 GB with an uncheckpointed WAL.

10. `resolve()`: from `IngestInput::Native { session_id, .. }`, one scoped query for the
    addressed session, and one for its children via the `external_parent_session_id`
    COLUMN. Measured: `agent-a01869bcb5e09448b` → `a5a5588f-...`, and it is a real
    column, not JSON, so no parsing is needed. Returns
    `SessionFile { path = THE DATABASE, session_id = external_session_id, kind,
    parent_session_id, harness: MetricsDb }` per u5. Re-queried on EVERY call, so a
    subagent that starts mid-session is found.

11. `read_incremental()`:
    `where rowid > :cursor and external_session_id = :sid and event_kind = 5 and <scope> order by rowid`.
    The cursor is `SourceCursor::RowId` ONLY; `ByteOffset` and `Seq` return
    `Error::Provider`, never read as zero (u4 plus contract case 6). An empty batch
    returns the cursor unchanged.

    **`rescanned` is FALSE always for this store**, and I will say why in the rustdoc:
    sqlite rows are never rewritten or truncated under a rowid, so there is no rotation
    to detect. If you want a rescan signal on a store PRUNE — `schema_metadata` carries
    `metrics_last_prune_ts`, and this database self-prunes — rule it, and I will detect
    `cursor > max(rowid)` as `rescanned` instead. FLAGGING rather than deciding.

12. Claude-mirror dialect: the 12-type bookkeeping allowlist; merge by
    `v."0".message.id`; `user` → Human/Human; `assistant` → Agent/System; `tool_use`
    and `tool_result` blocks → `TurnItem`. Recipe gotcha 5 compaction — id 945255,
    `type` = `user` with `isCompactSummary` = true — is KEPT, as `TurnSource::System`.

13. Copilot dialect per the Q6 ruling, dispatched off the store's own **`tool` column**
    (u3) — values `claude` (72 rows) and `github-copilot-cli` (28 rows) — never a
    hand-kept session-id list. *(This is the clause the shell corrupted.)*

14. Tests, `crates/providers/tests/`:
    - **(a)** the full contract suite via a `SourceFixture` over a **tempdir copy** of
      `metrics.sqlite3` (u6 and i6 — the committed bytes stay untouched, and
      `Expectations::verify_fixtures_unchanged` keeps proving it). `grow()` inserts real
      rows copied from the fixture under fresh rowids. `begin_partial_record` returns
      `false`.
    - **(b)** `Expectations::assert_ordinals_are_a_subsequence` for BOTH claude
      sessions, plus `assert_oracle_prose_appears` for the 3 prose turns of
      `a5a5588f` (`oracle_by_kind`: assistant 1, human 1, pij_in 1).
    - **(c)** SCOPING BY EXCLUSION: reading with scope = flowspace3 returns ZERO records
      for the three foreign sessions, and the 97/100 count holds.
    - **(d)** API-SHAPE test proving the unscoped call does not exist.
    - **(e)** copilot dialect case including the per-call model, with the seat-label
      trap `PROVENANCE.md` names: id 948627 reports `data.currentModel` =
      `gemini-3.7-flash` while the actual per-call model on 936664-936666 is
      `gpt-5.4-nano`. The label lies, so I read the per-call field.

15. `docs/services/convo-source-metricsdb.md`, including the SNAP-IN RECIPE (d5): the
    construction call with its `RepoScope` value, the one `pub mod` line, and exactly
    how u4 derives the remote URL from `IngestInput::folder`.

16. `harness checks` green in this worktree with `CARGO_INCREMENTAL=0` and
    `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/fs3_convo_u1d`.
    Done report with test names, counts, deviations, and observed rustc/cargo versions.

---

## Blocking on

**D3** and **Q4 through Q7**. Items 8-16 do not start until those are ruled.
