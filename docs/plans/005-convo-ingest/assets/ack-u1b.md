# ACK — u1b (omp reader + pij-ledger reader), plan 005-convo-ingest

**Seat**: `pij-suitable-cormac` · **Unit**: u1b · **Worktree**: `../fs3-convo-u1b` · **Branch**: `005-convo-u1b`
**PM**: pij-pale-silkworm (PM3) · **Written**: 2026-08-28 · **Disk at write**: 30Gi Avail

Pointer delivery, per PM3 standing procedure after DL-006 (a pij message is a
shell-expansion channel: backticks and `$(...)` in the body are EXECUTED by the
sending shell, and the corruption is silent on both ends). Nothing below is
pasted into a pij message.

This file supersedes the corrupted ack. Items 8, 13 and 14 are already ruled and
are restated here only so the record is complete.

---

## Ruling A — the measurement PM3 required

> "Measure whether any omp toolResult body is a PREVIEW or a POINTER to its
> `NN.bash.log` rather than the content itself."

**Answer: 1 of 72 toolResult records. It is a PREVIEW PLUS POINTER — never a bare
filename. But it is a lossy preview, and that is worse than the claude case.**

Measured on the committed bytes:

| claim | value |
| --- | --- |
| toolResult records in the fixture | 72 |
| bodies containing `artifact://` | **1** (record at line 138) |
| bodies that are a bare filename with no content | **0** |
| elision markers inside that one body | **2** — `[+503]` and `[+338]` |
| inline body length | 895 chars / 903 bytes |
| committed spill file `30.bash-original.log` | 2,070 bytes (sanitiser-capped from 3,949 source bytes) |

**The decisive finding: omp's inline preview is NOT a prefix of the spill file.**

- inline body begins: `commit 7975adc\ndocs(prd): req-0053 skill distribution...`
- spill file begins: `commit 7975adc405f09448d942831326477f6635f0fbc8\nAuthor: ...`

The abbreviated sha and the missing `Author:` line prove the inline text is a
*re-rendered, multi-point-elided* view, not a head-truncation. It has **two holes
in the MIDDLE** (`[+503]`, `[+338]`), not just a cut tail. Content that exists in
the store is absent from the inline body at positions a head-cap would never
reach.

### Symmetry with u1a's claude spill — measured, not assumed

I read the claude fixture (read-only; I edited nothing in u1a's fence) because
PM3 made symmetry the reason for the requirement.

| | claude (`b1d6f4fb`, line 24) | omp (line 138) |
| --- | --- | --- |
| marker | `<persisted-output>` + `Preview (first 2KB):` | `…[+338]\n[raw output: artifact://30]` |
| pointer form | **absolute filesystem path** | **opaque artifact id** `artifact://30` |
| spill file | `tool-results/b8e9hq4my.txt`, 34,821 bytes | `<session>/30.bash-original.log` |
| store states the true size | **yes** — "Output too large (34KB)" | **no** — `[+N]` counts an elision, not the total |
| preview is a literal prefix of the file | **yes** | **NO** |

So both stores are preview-plus-pointer, and the "stored as a filename" failure
mode PM3 was guarding against does not occur in either. **But omp is the weaker
of the two**: claude's preview is a faithful 2KB prefix and claude announces the
true size, whereas omp's preview is lossy in the middle and omp announces nothing
about the total.

### What I recommend, and why (item 7 is not yet ruled — this is a proposal)

**Resolve the body from the spill file when `[raw output: artifact://<n>]` is
present.** Three reasons, in order of weight:

1. **It is not a truncation, it is a hole.** A 512-byte head taken from the
   inline body and a 512-byte head taken from the raw file are *different text*
   (see the two openings above), so this is not a case where the payload policy
   makes the question moot. `OUTPUT_HEAD_BYTES = 512`
   (`crates/daemon/src/conversations.rs:58`) and both candidate bodies exceed it.
2. **Symmetry is the point.** If u1a resolves claude's persisted output and I do
   not resolve omp's, then the same `git show` result is searchable in one
   harness and not the other — PM3's "defect wearing a dialect's clothes",
   exactly.
3. **`total_bytes` is only honest from the file.** `[+338]` is an elision count,
   not a total; 895 + 338 = 1,233 against a 3,949-byte source. Deriving
   `total_bytes` inline would ship a number that is wrong by 3x.

**Resolution rule**: `artifact://<n>` maps to the first match of
`<session-dir>/<n>.*` — the extension varies in the real store
(`9,10,11,85` are `.bash.log`; `30,37,41,65` are `.bash-original.log`, per
PROVENANCE), so the numeric prefix is the artifact id and globbing on it is the
only sound lookup.

**Fallback, non-negotiable**: if the spill file is absent, fall back to the
inline body and mark `truncated = true`. A spill file can be garbage-collected,
and erroring there would make an entire conversation unreadable because one tool
result aged out.

**One caveat on what I can ASSERT**: the committed `30.bash-original.log` is
itself sanitiser-capped (2,070 of 3,949 bytes) and PROVENANCE warns explicitly
that it does not hold the store's complete output. So my test asserts the
*resolution behaviour* — that the body comes from the file, that it begins with
the full 40-char sha and the `Author:` line the inline body lacks, and that a
missing file degrades to the inline body — never a byte-exact total against real
store output. That claim would be false for these bytes and I will not write it.

---

## Items 1-7 and 9-12, restated intact for ruling

### Layout

**1.** Two files, two impls: `crates/providers/src/conversation_sources/omp.rs`
and `pij_ledger.rs`. Exactly two `pub mod` lines added to that directory's
`mod.rs`, kept alphabetical (`omp`, `pij_ledger`, `tail`). Both readers use
`tail::read_lines` for framing; I write no fourth tail buffer.

**2.** **No new dependencies.** `serde` and `serde_json` are already in
`fs3-providers`. For scratch dirs in tests I follow testkit's own precedent
(`fake_source.rs:250` — `std::env::temp_dir()` plus a nanos suffix) rather than
asking for `tempfile`, so my unit changes no `Cargo.toml` at all.

### omp reader

**3.** *(APPROVED)* `resolve()` for `IngestInput::Native{session_id, harness: Omp}`
globs `<root>/<slug>/*_<session_id>.jsonl`. The slug **strips the home prefix**
(measured correction 1): `-substrate-flowspace-flowspace3`, not claude's
`-Users-...` form. The sessions root is injected at construction so tests point
at a scratch dir. `resolve` re-globs on **every** call per the trait rustdoc.
Returns exactly one `SessionFile`, kind `Main`; spill sidecars are tool output,
not conversations. `expected_session_files() == 1`.

**4.** **Record allowlist — emit 117 of 193**: `message` 114 + `compaction` 1 +
`custom_message` 2.
Dropped, with reasons:
- `title` (line 0) — carries **no `id` field at all**, so it can have no
  ordinal; `oracle_expectations.py` keys omp structural on `id`, so it is
  already absent from `Expectations::ordinals`.
- `session`, `model_change`, `thinking_level_change` — none are turns.
- all 72 `custom` / `tool_execution_start` **mirrors** — the mirror is precisely
  what makes a naive tool count double (u3); the call itself lives in the
  assistant record.

Legal because the committed claim is a **subsequence**, not equality.

**5.** `ordinal` = the record's `id` field (8-hex handles, not uuids — see
PROVENANCE "further findings"). `parent_ordinal` = the record's `parentId`
field. `at` = the record-level `timestamp` field.

> Note, measured while writing this: on a `toolResult` the **inner
> `message.timestamp` is epoch-milliseconds** (`1787731213876`) while the
> **record-level `timestamp` is an ISO-8601 string**. `RawRecord::at` is
> specified as RFC 3339, so I take the record-level field. Keying on the inner
> one would silently produce integers where the contract wants timestamps.

Role and source mapping:

| record | role | source |
| --- | --- | --- |
| `message` role `user` | Human | Human |
| `message` role `user`, text starts `[pij from` | Human | **Peer** |
| `message` role `assistant` | Agent | System |
| `message` role `toolResult` | Agent | System |
| `custom_message` (`async-result`, attribution `agent`) | Agent | System |
| `compaction` | Agent | **System** |

The `[pij from` case is 10 of the 11 user records in this window. That is the
axis the oracle spells `pij_in` and the axis `TurnSource::Peer` exists for.

**6.** `body`: **measured — no omp record in this fixture carries more than one
non-empty text block** (user 11 records x 1, assistant 4 x 1 with 27 having
none, toolResult 72 x 1). So `body` is that single block's text, verbatim. No
merge rule is needed and there is no concatenation hazard against the oracle's
per-block hashes. `thinking` blocks are **not** body — the oracle drops them and
they are not prose the store attributes to model output.

**7.** `items`:
- assistant `toolCall` blocks map to `TurnItem::ToolCall { tool: <the block's
  "name" field>, input }`.
- `toolResult` records map to `TurnItem::ToolResult { tool: <the record's
  "toolName" field>, head, total_bytes, truncated }`, with the spill resolution
  proposed under Ruling A above.

u3 confirmed on the bytes: the name lives on the **call**; `toolName` appears
only on the **result** and on the mirror. All 72 toolResults carry
`toolCallId` + `toolName`, and all 72 pair to a call **exactly once** — zero
orphans in both directions. I keep a call-id to name index so a result is never
mis-attributed, and I assert the zero-orphan property as a named test.

**9.** `compaction` (u1, ac-0005): first-class record at line 184, **emitted,
never dropped**. `ordinal` `a932507b`, `parent_ordinal` `58a257ae`, role Agent,
source **System**, `body` = the `summary` field. It sits **in** the parent chain
— line 185's `parentId` equals its `id` — so dropping it also breaks the chain
across the seam. A named test asserts its presence by ordinal. Its absence from
the subset section is **by construction** (the oracle handles only
`type == "message"`) and I will not chase that as a divergence.

**10.** Cursor: `SourceCursor::ByteOffset` through `tail::read_lines`, which
already refuses `Seq`/`RowId`, reports `rescanned` on inode change or shrink, and
stops at the last newline. Line 0's 256-byte in-place title rewrite (u2) is
byte-stable, so offsets survive it. I never treat `size == offset` as "nothing
changed", and I never cache line 0's title.

### pij-ledger reader

**11.** `resolve()` for `IngestInput::Pij{id: seat}` maps to
`<root>/<seat>/events.ndjson` — one `SessionFile`, kind `Main`, harness
`PijLedger`. Root injected at construction for tests.
`Native{harness: PijLedger}` is accepted with `session_id` read as the seat; any
other harness is an `Err`.

**12.** Records: **all 50 emitted** (`message` 22, `tool_call` 13,
`tool_result` 13, `receipt` 2).

`ordinal` = the `seq` field rendered as a **decimal string** (`"118"` .. `"167"`).
That is exactly what `build_pij` pins — `jsonl_structural(..., id_key="seq")`
stringifies it — so any other spelling fails
`assert_ordinals_are_a_subsequence`.

`parent_ordinal` = `None`: the store keeps no chain, and the driver passes
`parent_key=None`. `at` = the `timestamp` field.

| record | role | source | items |
| --- | --- | --- | --- |
| `message` role `user` | Human | Human (Peer when `[pij from`) | — |
| `message` role `assistant` | Agent | System | — |
| `message` role `toolResult` / `custom` | Agent | System | — |
| `tool_call` | Agent | System | `ToolCall` from `toolName` + `input` |
| `tool_result` | Agent | System | `ToolResult` from `toolName` + `content` + `isError` |
| `receipt` | Agent | System | — (body renders `to` / `state` / `messageId`) |

The two receipts are seq 122 (`queued`, non-delivered) and seq 127
(`delivered`). A delivery receipt is a real record and is **emitted, not
dropped** — the ledger is the only store in the fleet that records delivery
state, which is why the fixture was harvested around them.

---

## Already ruled — restated for completeness

**8. APPROVED — the `xd://` rule keys on `arguments.path`, never on the tool
name.** Measured **5 occurrences, not 4**: four `write` **plus one `read`**, all
with `arguments.path == "xd://pij_send"`, at lines 93, 105 (twice in one
assistant record), 125 and 174. Any toolCall whose `arguments.path` starts with
`xd://` is an in-process tool invocation: `tool` = the path suffix after
`xd://` (`pij_send`), `input` = `Verbatim` over the arguments, **never**
`Elided`-as-a-write. A rule keyed on `name == "write"` misses the `read` at line
93 and reports a fictional file read. Per PM3, a named test asserts the
**property** so a future name-keyed rewrite fails loudly.

**13. ACCEPTED AS IS — pij cursor, no byte-offset fast path.**
`SourceCursor::Seq{seq}`. Each poll calls `read_lines(path, None)` for framing
(which drops any torn tail line), then filters `seq > held`. Cursor = max seq
seen, or the held seq when nothing is new, so an empty poll returns an identical
cursor. `rescanned` is **always false**: a Seq cursor survives a whole-file
rewrite, which is the point of the variant. A `ByteOffset` or `RowId` cursor
handed to this reader is an `Err`; conversely omp errors on `Seq` — contract case
6 in both directions (u6).

**14. APPROVED — real-prefix-seed plus real-tail-append for `grow()`.**
`SourceFixture` operates on a **scratch copy** (i6). The copy is seeded with a
real **prefix** of the committed file, and `grow()` appends the real **remaining
lines** — genuine store bytes in genuine store order, so "real records, not
invented ones" holds literally. `expected_records()` and the `grow()` count are
**computed from the bytes**, never hand-counted, so a fixture regeneration does
not silently invalidate them. omp: the prefix runs **through the compaction
seam**, putting the seam on the boundary rather than safely inside one side.
pij: prefix is 40 lines (covers **both** receipts, lines 5 and 10); `grow()`
appends the last 10. `begin_partial_record()` writes the real next line cut
mid-JSON with no newline and returns `true` for both stores;
`finish_partial_record()` completes it.

---

## Tests, docs, gate

**15.** Two contract runs: `conversation_source_contract` over an omp fixture and
over a pij fixture (d2). Plus a separate **read-only** test per store over the
full committed file: `Expectations::verify_fixtures_unchanged`,
`assert_ordinals_are_a_subsequence`, `assert_oracle_prose_appears`.

Measured targets:
- **omp** — 15 prose turns must match verbatim (assistant 4, human 1, pij_in
  10). `pij_out` 7 and `tool_call` 65 are held only to their counts, since they
  are not in `prose_kinds`.
- **pij** — exactly **1** prose turn (`pij_in`). This is why I treat the
  **structural** section as this store's real done-bar (u5) and will not report
  a green subset as proof of anything.

**16.** Named tests beyond the suite: zero-orphan toolCall/toolResult pairing
(u3); the `xd://` remap keyed on the path property, covering the `read` at line
93 (u4, ruled); compaction present with its parent chain intact (u1 / ac-0005);
foreign-cursor refusal in both directions (u6); rescan on inode change; the
title-line rewrite not disturbing the offset (u2); spill resolution and its
missing-file fallback (Ruling A, pending item 7).

**17.** My unit is offline and needs no Postgres, but I run the gate with
`FS3_TEST_DATABASE_URL` pointed at `fs3_convo_u1b` on `127.0.0.1:5433`. I never
run `docker compose up` (i5).

**18.** `docs/services/convo-source-omp.md` covers **both** readers and carries
the d5 **snap-in recipe**: exact construction calls, the two `pub mod` lines,
config shape (omp sessions root, pij root), and precisely what u4 pastes at the
composition root. I write the recipe; I never wire it in myself.

**19.** `harness commit` as I go, conventional commits (`feat`, scope
providers). `harness checks` green in my own worktree before I claim done.
`CARGO_INCREMENTAL=0` throughout. I stop and tell PM3 on `rustc-LLVM ERROR: IO
failure on output stream` rather than debugging it as a compile error, and I
escalate mid-step if Avail falls below ~15Gi. Done report carries rustc/cargo
versions, test names and counts, every deviation and why, and the disk figure.
Then I hold. I never merge, never open a PR, never touch main.

---

## Open for ruling

- **Items 1-7 and 9-12** above.
- **Ruling A follow-up**: resolve the omp spill body from `<n>.*` in the session
  directory (my recommendation, with the missing-file fallback), or leave the
  lossy inline body as-is. This lands inside item 7.

No contract change is requested. Nothing in items 1-19 needs a method the frozen
`ConversationSource` trait does not have.
