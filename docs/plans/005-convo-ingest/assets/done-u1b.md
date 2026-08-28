# DONE — u1b (omp reader + pij-ledger reader), plan 005-convo-ingest

**Seat** `pij-suitable-cormac` · **Branch** `005-convo-u1b` · **2026-08-28**
**Gate** `harness checks` → **VERDICT ok**, all 9 gates PASS, in my own worktree
**Disk at write** 280Gi Avail · **rustc** 1.95.0 (59807616e 2026-04-14) Homebrew · **cargo** 1.95.0 (f2d3ce0bd 2026-03-21) Homebrew

---

## What shipped

| path | what |
| --- | --- |
| `crates/providers/src/conversation_sources/omp.rs` | `OmpSource` — full `ConversationSource` impl |
| `crates/providers/src/conversation_sources/pij_ledger.rs` | `PijLedgerSource` — full `ConversationSource` impl |
| `crates/providers/src/conversation_sources/mod.rs` | exactly two `pub mod` lines |
| `crates/providers/tests/conversation_sources.rs` | contract suites, expectation claims, named claims |
| `docs/services/convo-source-omp.md` | both readers + the snap-in recipe |

**No `Cargo.toml` anywhere was touched. No dependency was added.** Scratch dirs
follow testkit's own precedent (`fake_source.rs:250`) instead of `tempfile`.

Commits: `f7193222` (readers + tests + page), `c9c4a78` (opaque-ordinal ruling),
`4142b3e` (the ack).

## Evidence

| suite | count |
| --- | --- |
| `omp` module unit tests | **20** |
| `pij_ledger` module unit tests | **18** |
| `tests/conversation_sources.rs` integration | **14** |
| **my unit total** | **52** |
| workspace `cargo test --all` | **782 passed, 0 failed** |

Gates: `docs` `lock` `testdb` `fmt` `clippy` `prodguard:before` `test`
`prodguard:after` `arch` — all PASS, `FS3_TEST_DATABASE_URL` on `fs3_convo_u1b`.

### The done-bar tests, by name

- `the_omp_reader_satisfies_the_contract` / `the_pij_ledger_reader_satisfies_the_contract`
  — `fs3_testkit::conversation_source_contract`, all five cases plus the foreign-cursor case.
- `omp_ordinals_are_a_subsequence_of_the_store` / `pij_ordinals_are_a_subsequence_of_the_store`
  — the structural done-bar.
- `every_omp_oracle_prose_turn_appears` (15 prose turns verbatim: 4 assistant, 1 human, 10 pij_in)
  / `every_pij_oracle_prose_turn_appears` (**1** prose turn — deliberately weak, see assumptions).
- `the_committed_omp_fixture_is_unchanged` / `the_committed_pij_fixture_is_unchanged`.
- `the_compaction_record_is_never_dropped` — ac-0005, plus the parent chain across the seam.
- `an_xd_tool_call_is_never_reported_as_a_file_operation` — asserts **5**, which a name-keyed rule scores 4.
- `a_spilled_tool_result_is_resolved_from_its_artifact_file` — full 40-char sha + `Author:` line.
- `every_tool_result_pairs_with_exactly_one_call` — 72/72, zero orphans both directions.
- `a_foreign_cursor_is_refused_by_both_readers`, `resolve_stamps_every_file_with_its_own_store`.
- `a_tool_result_takes_the_record_level_iso_timestamp`, `every_text_block_survives_not_just_the_first`
  (both readers), `the_ordinal_is_the_decimal_string_form_of_seq`, `the_receipt_rendering_is_pinned`,
  `an_unknown_record_type_is_dropped_not_fatal` (both readers).

## Deviations from the packet

1. **Item 6 rewritten on your ruling.** Packet-era plan said body = the single
   text block. Now a fold over N blocks, both readers, with a named two-block
   test. You were right that my measurement was sound and my conclusion was not.
2. **Item 8 widened against the packet, with evidence.** `xd://` keys on
   `arguments.path`, not `name == "write"`. Five occurrences, not four.
3. **Ruling A added work the packet did not name** — spill resolution from the
   artifact file, with fallback.
4. **`grow()` seeds a real prefix and appends real lines**; the packet was silent.
5. **Worktree already existed**; packet said create it.
6. **PM is you, not the seat the packet names.**

Nothing else. No contract change was requested or needed.

---

## ASSUMPTIONS — what I believe that I did not prove

You asked for real effort here. These are ranked by what they would cost if wrong.

### A1 — I assume omp never emits two conversations into one file

`resolve` returns exactly one `SessionFile` and the reader treats the whole file
as one conversation. **Not proven**: I verified the fixture has three *chain
roots* (`title` has no id; `session` and the first `model_change` have null or
absent `parentId`), which I read as header records rather than as separate
conversations. If omp ever appends a second session into one file, my reader
merges two conversations silently and the parent chain is the only evidence.
**Cost if wrong:** two conversations become one, unrecoverably, and no test fails.

### A2 — I assume `custom_message` is a turn worth keeping

Both instances in the window are `customType: "async-result"` with
`attribution: "agent"` — a background job reporting completion. I map them
Agent/System. **Not proven**: `custom_message` may carry customTypes that are
not turns at all. A wrong call here adds noise; it does not lose data. Low cost,
stated because it is the drop/keep decision I am least sure of.

### A3 — I assume the pij ledger's message blocks and its `tool_call` events describe the SAME tools

I map items from the dedicated events and deliberately ignore the assistant
message's `toolCall` blocks, to avoid doubling. **Not proven that they always
correspond.** If the ledger ever writes a message-block tool with no matching
event, I drop it silently. I did not verify the pairing on the pij side the way
I did for omp — the ledger's 13 `tool_call` and 13 `tool_result` events match in
count, which is suggestive and is not proof of pairing by id.
**Cost if wrong:** silently missing tool calls. This is the assumption I would
test first if given another hour.

### A4 — I assume the artifact marker is always the LAST such reference in a body

`artifact_reference` uses `rsplit_once`. One marker exists in the window. A body
containing two would resolve to the second. **Cost if wrong:** wrong spill file
resolved for that one result.

### A5 — I assume the spill file is UTF-8

Resolution uses `read_to_string` and falls back to the inline body on failure. A
binary spill degrades to the inline preview rather than erroring — deliberate,
but it means a binary tool output silently loses the resolution you ruled for.

### A6 — the peer heuristic is a convention, and I know it

Documented as such, falls through to Human. Stated here because it is the one
place where my output changes based on text a human could type by accident: a
person who literally types `[pij from …` is recorded as a peer turn.

### A7 — the oracle prose check proves almost nothing for pij

**One** prose turn out of 50 records. I have written this in the service page in
those words. If the pij reader is wrong about roles, sources or bodies, my test
suite would very likely still be green. The structural claim and the named
receipt/cursor tests are what actually hold this store.

### A8 — I did not prove the spill file is complete, and said so

The committed `30.bash-original.log` is sanitiser-capped (2,070 of 3,949 bytes).
My test asserts resolution *behaviour*, never a byte-exact total. A future reader
who adds a `total_bytes == 3949` assertion will write a test whose claim is
untrue of the bytes it runs on.

---

## For the composer

- `resolve` **must** be called every poll; the trait says so and omp honours it.
- `OmpSource::new` needs **home as well as** the sessions root — the slug is
  derived by stripping home. Wrong home ⇒ resolve error naming the slug rule.
- `OmpSource` **errors on `IngestInput::Pij`**: the seat→session join is yours.
- `rescanned: true` ⇒ records are the WHOLE file. Dedupe on `ordinal`. The pij
  reader never sets it; omp sets it on inode change or truncation.
- Both readers **refuse** each other's cursor variants. That refusal is an error
  the orchestrator must not swallow.
- Full construction, config keys and registration: the snap-in recipe section of
  `docs/services/convo-source-omp.md`. I wrote it; I did not wire it.

## Attribution data point (no action, per your instruction)

Third independent seat, and it eliminates one more theory. All three of my
commits reported `direct-verified` / `verify: landed`:

| commit | content | note shape |
| --- | --- | --- |
| `f7193222` | the readers, tests, service page | **agent** — `s_…::t_…` with per-file hunk ranges |
| `4142b3e` | `ack-u1b.md`, write tool | **human** `h_9e71e8b09f7cf2`, lines 1-319 |
| `c9c4a78` | service-page edit, edit tool | **human** `h_9e71e8b09f7cf2`, lines 39-73 |

Note `omp.rs` in the *correct* commit was also written with the write tool
(`s_…::t_… 1-791`), which independently confirms your finding that the
write-vs-edit theory is dead. In my data the split falls on **source vs
documentation**, but with n=3 that is a coincidence I am reporting, not a theory
I am proposing. I repaired nothing.

## Frictions filed (buffer only — I did not clear)

- **DL-001** `pij send` has no `--file`/stdin; shell expansion silently mangled a
  10KB message and pij delivered the garbage with a success receipt.
- **DL-002** file tooling resolves relative paths against the session cwd (the
  main clone), not the worktree — the cross-worktree write hazard.

One more worth encoding, not yet filed as its own record: a real omp compaction
summary opens with a markdown heading, so its JSON contains `"##` and closes
both `r#"` and `r##"` Rust raw strings. Cost me two compile cycles. Any seat
embedding real omp compaction bytes in a test needs `r###"`.

---

**HOLDING** for composition questions. I have not merged, opened a PR, or
touched main.
