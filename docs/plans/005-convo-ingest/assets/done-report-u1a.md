# Done report — u1a, claude-native reader (plan 005-convo-ingest)

**Seat** pij-frightened-mastodon · **Branch** `005-convo-u1a` · **Commit**
`3ce0332` · **Date** 2026-08-28

- rustc 1.95.0 (59807616e 2026-04-14) (Homebrew)
- cargo 1.95.0 (f2d3ce0bd 2026-03-21) (Homebrew)
- harness 0.13.0
- Disk at close: 36Gi avail on `/System/Volumes/Data`

---

## 1. What shipped

| path | what |
| --- | --- |
| `crates/providers/src/conversation_sources/claude.rs` | the reader (new, 25.5 KB) |
| `crates/providers/src/conversation_sources/mod.rs` | exactly one line: `pub mod claude;`, alphabetical |
| `crates/providers/tests/conversation_source_claude.rs` | 15 tests (new, 17.6 KB) |
| `docs/services/convo-source-claude.md` | service page + snap-in recipe (new) |

Nothing else was touched. No dependency was added, no migration, no change to
the frozen contract, no other unit's paths.

## 2. Evidence

`harness checks` — **GREEN** in my own worktree, with
`FS3_TEST_DATABASE_URL` pointed at `fs3_convo_u1a`. All gates ok: docs, lock,
testdb, fmt, clippy `-D warnings`, prodguard before/after, and
`cargo test --all`.

`cargo test -p fs3-providers --test conversation_source_claude` — **15 passed,
0 failed**:

- `the_claude_reader_satisfies_the_conversation_source_contract` — the shared
  five-case suite over a scratch copy
- `emitted_ordinals_are_a_subsequence_of_what_the_store_holds`
- `the_committed_fixtures_are_unchanged`
- `assistant_blocks_merge_by_message_id_to_the_count_the_fixture_pins`
- `a_message_interrupted_by_its_own_tool_result_stays_one_turn` (mutation-checked)
- `a_merged_turn_is_reported_under_its_first_block_not_its_last`
- `an_unknown_record_type_is_dropped_and_its_neighbours_still_parse`
- `store_metadata_rows_are_not_turns`
- `a_spilled_tool_result_is_resolved_to_its_full_bytes`
- `an_unresolvable_spill_falls_back_to_the_preview_and_says_it_is_short`
- `resolve_finds_the_sidecar_and_names_its_parent`
- `a_sidecar_that_appears_mid_session_is_found_by_the_next_resolve`
- `a_session_this_store_does_not_hold_is_refused`
- `a_pij_seat_is_refused_rather_than_joined_here`
- `a_foreign_cursor_is_refused`

Measured shape: the main fixture's 148 store records become **39 turns** (13
merged assistant + 26 user); `b1d6f4fb`'s 35 become 9.

**The mutation check was performed, not claimed.** Changing the merge to an
adjacent-run fold (closing open groups at each `user` record) produced 20
assistant turns instead of 13 and failed 4 tests. Worth recording:
`emitted_ordinals_are_a_subsequence` still **passed** under the broken
implementation — 20 ordinals are still a valid subsequence — so the structural
done-bar alone would NOT have caught the bug the packet was most worried about.
The shape guard is what catches it.

## 3. ASSUMPTIONS — read this section

Things I decided that someone else's code depends on, or that are true but not
obvious. Each is a place I could be wrong.

**A1. Ordinal = the first line `uuid` of a merged group.** Ruled by PM, stated
as a FROZEN FACT at the top of the service page with the consequence spelled
out: changing the derivation makes every stored record look new and silently
doubles the conversation, unrecoverably. u2 consumes this as its dedupe key.

**A2. A group split across polls yields two turns, permanently.** Ruled by PM.
The second half is stored under its own ordinal and the first is never
backfilled — not a delay, a permanent split. One assistant message then reads
as two turns. u2's normaliser and anyone reading a transcript will see this.

**A3. This reader is LOSSLESS; the payload policy is the normaliser's.** I
emit `thinking` and `text` verbatim, tool inputs whole, and resolved spills at
full bytes with `truncated: false`. I based this on the impl-guide's own
division of labour ("apply the payload policy, drop what v1 does not store").
**If u2 assumed the reader had already truncated to 512 B or elided
write-family bodies, we disagree and one of us must change** — flagging
explicitly because `ToolInput::Elided` exists in the frozen contract and this
reader never constructs it. Cheap to move if ruled the other way.

**A4. The payload spec's claim that `thinking` blocks are "absent in claude
data anyway" is FALSE.** Measured on the committed fixtures: 21 `thinking`
blocks against 5 `text` blocks. Whoever applies the drop rule is discarding the
majority of assistant prose for this store, not a no-op. The rule may still be
right; its stated justification is not.

**A5. A tool result's tool NAME is resolved within the batch, falling back to
the `tool_use_id`.** A `tool_result` record names only the id of the call it
answers, and `toolUseResult` never carries the name (checked across every
fixture). Calls and results are adjacent in practice, so the map almost always
hits — but if a poll lands exactly between them, that result's `tool` field is
the id rather than e.g. `Bash`. Consequence: the same record read incrementally
vs. read in a rescan can differ in that one field. It does not affect the
ordinal or the dedupe. I judged a stable, honest id better than inventing a
name; say the word if you want it carried across polls instead, which would
need reader-side state that the contract has nowhere to put.

**A6. `TurnSource::Peer` is only reported when `origin.kind` says so.** Claude's
store has no signal distinguishing a pij-injected turn from a typed one — they
are byte-identical — so the fleet-composition question recipe gotcha 5 cares
about is, for THIS store, unanswerable from the bytes. Most user records carry
no `origin` at all (30 of 33 in the fixtures) and default to `Human`. If the
plan needs peer attribution for claude sessions, it must come from elsewhere;
I refused to guess it from message text.

**A7. `head_sha` is always `None`.** The records carry `gitBranch` but no commit
sha anywhere. A branch name is not what `head_sha` promises.

**A8. Spills resolve by FILE NAME, never by the stored absolute path.**
`persistedOutputPath` is absolute on the machine that wrote it
(`/Users/agent/...` in the fixture) and does not exist on the reading machine.
Only the basename is portable. If a future claude version spills somewhere
other than `<session>/tool-results/`, resolution silently falls back to the
preview rather than failing — deliberate, but it means a silent quality drop is
possible where a loud failure is not.

**A9. `resolve()` refuses `IngestInput::Pij`.** The seat-to-session join is the
orchestrator's. u4 must do the join before calling this reader.

**A10. `ClaudeSource::new` takes the project directory, not a slug or a home
dir.** The reader never derives the `-Users-...` slug. u4 owns that resolution.
This is what lets the tests run against a scratch directory.

## 4. Deviations from the packet

1. **u5's record-type enumeration was wrong** — the packet lists 13 types from
   session `a5a5588f` only; the union across both sessions plus the sidecar is
   14 (`ai-title` missing). Ruled: the allowlist is a BEHAVIOUR, not a list.
   Unknown types drop silently, with a named test.
2. **u2's merge description was incomplete** — the groups are not adjacent runs.
   Interleaved `user` tool_result records split them (20 runs vs 13 ids). Ruled:
   keyed grouping over the assistant projection.
3. **The impl-guide's u1a row says `cargo test -p fs3-parsers`** — stale, SA1
   moved readers to providers. Ran `-p fs3-providers`. PM owns the correction.

## 5. What the composer must know

- One line to add, already present and alphabetical: `pub mod claude;`.
- `ClaudeSource::new(<projects dir>)`; blocking, hand it to `spawn_blocking`.
- It accepts only `IngestInput::Native { harness: Claude, .. }`.
- A5 and A3 are the two places my output shape could disagree with u2's
  expectations. Both are cheap to change now and expensive after u2's ledger has
  persisted records.

## 6. Process notes worth keeping

- **My first green build proved nothing.** `cargo build -p fs3-providers`
  exited 0 while my own `mod.rs` still lacked `pub mod claude;`, so a 25 KB file
  was never compiled. The registration edit had gone to the PM's tree via a
  relative path. Filed as DL-008. The decisive proof afterwards was not an exit
  code but that the test binary imports
  `fs3_providers::conversation_sources::claude::ClaudeSource` — which fails to
  compile when unregistered, and now passes 15 tests.
- **Content-derived edit tags cannot distinguish two trees.** My `mod.rs` and
  the PM's were byte-identical and hashed to the same tag; re-reading to get a
  "fresh" tag returned the same one. The symptom is inverted — the edit reports
  success, echoes correct content, then appears to have reverted — which reads
  as a flaky tool rather than a misdirected write. Absolute paths always.

---

# Revision 2 — after the A3/A4/A5 rulings (2026-08-28)

`harness checks` **green** again: docs, lock, testdb, fmt, clippy, prodguard
×2, `cargo test --all`, arch. **19 tests** (was 15). Disk 279Gi avail.

## What changed

**A3 — no change.** Confirmed correct: payload policy is the normaliser's, per
the frozen contract's own rustdoc. This reader never mints `ToolInput::Elided`
and never truncates. u2's `shape_turn` owns it.

**A4 — thinking is now DROPPED at the reader** (prime ruling, option A),
matching the omp reader. The structural reason is the load-bearing one: a
block's type survives only until blocks are concatenated into one body string,
so the rule is only implementable at the reader.

**A5 — documented**, as required: the tool-name fallback is permanent, because
a rescan that could resolve the real name is deduped away. Stated in the
service page as a fact beside the split-group case.

**Required addition 1 — the distinguishing test shipped.**
`an_adjacent_run_fold_cannot_pass_this` pins the count of *distinct assistant
ordinals* and names the continuation blocks that must never become ordinals.
Re-verified by re-running the mutation after the thinking change: it fails,
along with 4 others, and `emitted_ordinals_are_a_subsequence` **still passes** —
the done-bar gap is reproducible, not incidental.

**Required addition 2 — the grouping rule is frozen**, in both the service page
and the module docs: *a group is every `assistant` record sharing one
`message.id`; membership is decided by record type and `message.id` alone,
never by content blocks and never by payload policy*, with the silent-doubling
consequence spelled out.

## NEW MEASUREMENT — the A4 justification is wrong, and part of that is mine

**Claude does not persist thinking TEXT at all.** All 21 thinking blocks in the
committed fixtures carry an encrypted `signature` of 452-2068 bytes and a
`thinking` field of length **zero** — 0 bytes of reasoning prose across the
whole fixture set. Not a harvest artefact: provenance records 0 credential
redactions, and its body cap leaves a `…[fixture-truncated]` suffix rather than
an empty string.

So dropping thinking removes 21 EMPTY blocks from this store and saves no index
bytes and no embed spend. The cost/noise justification does not hold for claude.

**My error, owned:** I reported "21 thinking blocks against 5 text blocks" and
let it read as prose *volume*. It is a count of BLOCKS. I never measured bytes,
and the bytes are zero. The block count is correct and was independently
reproduced, but it does not support the cost conclusion drawn from it.

The ruling still lands on option A, for reasons that survive: structural
necessity, cross-harness consistency with u1b, and safety if Anthropic ever
begins persisting the text. Recommend the vendored spec record it that way and
drop the cost claim for claude.

`claude_does_not_persist_thinking_text` pins 21 blocks / 0 bytes so this cannot
rot. Note the consequence for testing: the drop rule itself must be proved from
a **synthetic** session, because the committed fixtures contain no reasoning for
a broken reader to leak.

## Assumption A11 (new)

**Dropping thinking discards a block's TEXT, never its line's group
MEMBERSHIP.** The first block of a group is routinely a thinking block
(`9ccf07af` in the fixture), so a reader that skipped thinking *lines* would
move that group's ordinal to `82ab2abe` and silently double every stored claude
conversation. `dropping_thinking_does_not_move_an_ordinal` guards it. This is
the concrete instance of u2's frozen-grouping-rule warning.

---

# Revision 3 — F-A1 and the scratch-dir hardening (2026-08-28)

`harness checks` green. **20 tests** (was 19).

## F-A1 — a record with no `uuid` is refused, not defaulted

Fixed as u2 asked, and fixed structurally rather than locally: `Line::uuid` is
no longer an `Option`, so serde refuses such a line and `parse_lines` drops it
like any other unreadable one. `unwrap_or_default` is gone from the ordinal
path entirely.

**Honest reachability note.** The hazard was already unreachable in the shipped
code: `parse_lines` filtered `uuid.is_some()` before any record was built, so no
empty ordinal could be produced. What was wrong is what u2 identified — the
invariant was enforced by a filter several functions away from the
`unwrap_or_default` that depended on it, so a later edit removing the filter
would have reintroduced silent, durable turn loss with nothing to catch it. The
fix moves the invariant into the type, where it cannot be removed by accident.

**Verified by regression, not asserted.** Restoring the exact defaulting shape
(`uuid: Option<String>` + `unwrap_or_default`) makes the new test fail with
`["ok-1", "", "", "ok-2"]` — two colliding empty ordinals, precisely the ledger
poisoning u2 predicted. The test uses *two* uuid-less records for that reason:
one alone could not demonstrate the collision.

## Scratch directories are now unique by construction

`Scratch::new` adds a process-static `AtomicUsize` alongside the label and the
nanosecond stamp. Uniqueness was previously a property of every caller
remembering to pass a distinct label; it is now a property of the helper, so a
test added later by copy-paste cannot silently share a directory with another.

## Final state

20 tests, `harness checks` green, four commits on `005-convo-u1a`. Holding.
