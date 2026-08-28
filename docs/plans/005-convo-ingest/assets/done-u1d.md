# Done report — u1d, git-ai metrics-db reader

**Seat**: pij-causal-mollusk · **Branch**: `005-convo-u1d` · **HEAD**: `bf23f2d`
**PM**: pij-pale-silkworm (PM3) · **Date**: 2026-08-28 · **Avail**: 280Gi

---

## 1. Verdict

The unit is done to its own done-bar. `harness checks` is **green in my own
worktree** — all eight gates: `docs`, `lock`, `testdb`, `fmt`, `clippy`,
`prodguard:before`, `test`, `prodguard:after` — with
`FS3_TEST_DATABASE_URL` set to `fs3_convo_u1d` and `CARGO_INCREMENTAL=0`.

I never merged, never opened a PR, never touched main, and never touched another
seat's paths after the path incident was corrected.

## 2. Shipped

| commit | what |
| --- | --- |
| `27c9910` | canonical ack (the file you ruled against) |
| `23a8d1e` | the reader, the dependency, the test suite |
| `bf23f2d` | the service page and snap-in recipe |

| file | lines | note |
| --- | --- | --- |
| `crates/providers/src/conversation_sources/metrics_db.rs` | 972 | new, the whole unit |
| `crates/providers/tests/conversation_source_metrics_db.rs` | 548 | new, 16 tests |
| `docs/services/convo-source-metricsdb.md` | 366 | new, includes the snap-in recipe |
| `crates/providers/src/conversation_sources/mod.rs` | +1 | exactly the one `pub mod` line |
| `Cargo.toml` | +16 | the granted `rusqlite` row + rationale |
| `crates/providers/Cargo.toml` | +4 | `rusqlite.workspace = true` + comment |
| `Cargo.lock` | +42/-1 | `rusqlite 0.32.1`, one `libsqlite3-sys 0.30.1` |

Nothing outside the fence as you extended it. No second dependency.

## 3. Evidence

**Environment**: `rustc 1.95.0 (59807616e 2026-04-14) (Homebrew)`,
`cargo 1.95.0 (f2d3ce0bd 2026-03-21) (Homebrew)`.

**21 tests, all passing, all offline**: 16 integration + 5 module unit tests.

| claim | test |
| --- | --- |
| the shared contract suite | `the_reader_satisfies_the_shared_contract` |
| committed fixtures unchanged | `the_committed_fixtures_are_unchanged` |
| ordinals are a subsequence (both claude sessions) | `emitted_ordinals_are_a_subsequence_of_what_the_store_holds` |
| oracle prose, verbatim and in order | `the_oracle_prose_appears_verbatim_and_in_order` |
| merge arithmetic: 16 and 10, 13 assistant rows → 7 turns | `the_two_sessions_yield_the_records_the_merge_arithmetic_predicts` |
| compaction kept, marked `System` | `a_compaction_summary_is_kept_and_marked_as_written_by_the_harness` |
| injected packet is `Peer`, not `Human` | `an_injected_peer_packet_is_not_reported_as_a_human_turn` |
| scoping by exclusion, all 3 foreign sessions | `a_foreign_repo_session_is_invisible_to_a_scoped_reader` |
| scoping cross-check, 97 of 100 both ways | `the_fixtures_own_ninety_seven_of_one_hundred_claim_still_holds` |
| scoping by API shape | `the_unscoped_read_has_no_spelling` |
| copilot dialect off the `tool` column | `the_copilot_dialect_is_read_from_the_stores_own_tool_column` |
| copilot call/result pair on one turn | `a_copilot_tool_call_and_its_result_land_on_one_turn` |
| unknown event type + unknown tool dropped, cursor still advances | `an_event_type_this_reader_has_never_heard_of_is_dropped_not_fatal` |
| prune → rescan, whole conversation | `a_pruned_store_is_reported_as_a_rescan_rather_than_going_quiet` |
| empty scope ≠ prune | `a_session_with_no_rows_in_scope_is_not_mistaken_for_a_prune` |
| subagent names its parent | `the_subagent_names_its_parent_so_its_work_is_not_invisible` |
| foreign cursor refused | `a_foreign_cursor_is_refused_rather_than_read_as_zero` |
| dialect from the tool column, unknown tool → drop | `dialect_comes_from_the_tool_column_and_an_unknown_tool_is_a_drop` |
| epoch → RFC 3339, incl. a leap day | `epoch_seconds_render_as_rfc3339_utc` |
| URI-syntax characters in a db path escaped | `a_database_path_containing_uri_syntax_is_escaped` |
| prose joins without leading/doubled separators | `prose_joins_without_leading_or_doubled_separators` |

### The suite was mutation-checked, not merely observed green

It passed 16/16 on the first run, which is exactly when a suite deserves least
trust. Two mutations:

1. **Stop merging `message.id` groups** → 3 tests fail, including the shared
   contract (`a read from no cursor must yield the whole conversation`).
2. **Drop the repo predicate from the scoped query** →
   `a_foreign_repo_session_is_invisible_to_a_scoped_reader` fails.

Original restored and re-verified green afterwards.

### What the committed expectations do NOT catch — worth your attention

Under the no-merge mutation, **both** `assert_ordinals_are_a_subsequence` and
`assert_oracle_prose_appears` still **passed**. 22 records arrived where 16 were
correct and neither committed expectation noticed.

That is not a defect in them — a subsequence claim catches an invented,
reordered or duplicated ordinal, which is what it advertises. But it means the
merge arithmetic for the two group-derived readers is held **only** by each
unit's own count test and the contract's `expected_records`. If you want that
independently pinned at composition, it needs a number in a shared expectation,
not another structural claim.

## 4. Deviations from the packet

| # | packet said | shipped | why |
| --- | --- | --- | --- |
| D1 | copilot event name at `v."0".name` | reads `v."0".type` for both dialects | zero rows have `.name`; PROVENANCE, the frozen contract's rustdoc, and a `json_each` sweep all agree. You confirmed it a typo. |
| D2 | scope via `event_json LIKE '%flowspace3%'` | equality on `$.a."1"` | the `LIKE` is a substring search over conversation prose. Kept as a second test assertion, where it is a fine tripwire. |
| D3 | rusqlite row in `providers/Cargo.toml` only | also the root `[workspace.dependencies]` row | you extended the fence; the workspace has zero direct-version rows. |
| — | rusqlite version unspecified | pinned **0.32** | see assumption A4 — the version is not free. |
| — | `tempfile` for the scratch copy | `std::env::temp_dir` + `Drop` | you ruled: no second dependency edge. |

## 5. ASSUMPTIONS — read this section properly

Written to the standard you set: what I assumed, not what I built.

**A1 — `thinking` blocks are dropped entirely, and this is a content decision I
made inside my fence.** They do not reach `body` and they are not a `TurnItem`,
so the model's reasoning text is **not indexed** from this store. My reasoning:
the reference oracle does not render them, and the committed expectation
compares an assistant body by sha256 — so including them would make agreement
with the oracle accidental rather than definitional, and would break the moment
a fixture regeneration produced a group with both `thinking` and `text` blocks
(this fixture has none, which is luck, not design). **If the product wants
reasoning indexed, this is the line to change and it is a one-line change** —
but it changes stored bodies, so it must happen before first light, not after.

**A2 — the `Peer` turn source is detected from the body text, not a store
flag.** A user turn whose body starts with `[pij from ` is `TurnSource::Peer`.
The store records no flag for an injected packet, so there is nothing structural
to key on. If the fleet's wire format changes, peer turns silently become
`Human` turns and the "orchestrated fleet reported as half-human" failure
(workshop 005, C8) comes back with no error. This is the weakest inference in
the unit.

**A3 — the copilot allowlist is unfalsifiable by the committed expectations.**
`oracle_turns: 0` for that dialect. My five-record output for session `222c2c9d`
is pinned by my own test and by nothing else. A wrong allowlist would pass every
committed expectation. Labelled PM-derived-not-oracle in the service page. The
independent check has to be first light.

**A4 — the rusqlite version is pinned by `sqlx`, not chosen.** `libsqlite3-sys`
declares `links = "sqlite3"` and cargo permits one such package per graph;
`sqlx-sqlite` already contributes `libsqlite3-sys 0.30.1`, so rusqlite must be
0.32. Any other version makes the resolver walk `sqlx` backwards and fail on a
missing `tls-rustls-ring` feature — **an error that reads as an unrelated TLS
problem and sends the next person entirely the wrong way**. Documented in both
the manifest comment and the service page. If anyone bumps `sqlx`, these two
move together.

**A5 — `head_sha` is always `None`.** This store records a repo remote and a git
branch, never a HEAD sha. I checked the whole `a` envelope: keys `0` (version),
`1` (remote), `20` (tool), `23`/`24`/`25` (session/trace ids), `26`/`27` (on 15
rows). No sha anywhere. If turn-level `head_sha` matters for this store, it must
come from the orchestrator.

**A6 — a read-only open of a WAL database needs a writable directory** for the
`-shm` file. That is sqlite's behaviour, not mine. The fixture is
`journal_mode=DELETE` so no test exercises it; the **live** store had a 47 MB
uncheckpointed WAL at harvest. If first light runs as a user who cannot write
git-ai's directory, the open fails — loudly, which is the right direction, but
it will look like a permissions bug rather than a WAL one.

**A7 — the split-turn permanence is now shipped behaviour** (your Q7 ruling (i)).
A `message.id` group straddling a poll boundary is stored as two turns,
permanently, never backfilled. In the service page with the consequence traced.

**A8 — `resolve` refuses `IngestInput::Pij`.** The seat→session join is the
orchestrator's, per the contract's own rustdoc. If you expected this reader to
do the join, that is a real disagreement and I would rather hear it now.

## 6. For the composer

- The snap-in recipe is §"Snap-in recipe" of `docs/services/convo-source-metricsdb.md`.
- **What I need from you**: `RepoScope::remote_url(<remote as the tool recorded
  it>)` — scheme and host included, no `.git`, no trailing slash, exact string
  equality against `$.a."1"`. On the harvested machine,
  `https://github.com/AI-Substrate/flowspace3`.
- **The two cases I flagged as yours**: a folder with *no* remote (fail the
  ingest; there is no safe unscoped read of a machine-wide store, and a
  directory-name fallback is not a scope), and a folder with *several* (pick
  deliberately and record which — a fork's `upstream` is a different repository
  here; guessing wrong silently indexes another project and looks like missing
  data). An empty scope string matches nothing, which is the safe direction.
- The reader is `Send + Sync` and blocking. `spawn_blocking`, as with the ONNX
  embedder.
- An out-of-scope session is an **error** from `resolve`, not an empty result.

## 7. Attribution — third independent reproduction, no action taken

I checked my three commits read-only, per your instruction not to repair.

| commit | reported | note contents |
| --- | --- | --- |
| `27c9910` | `direct-verified`, `verify: landed` | `"humans"`, **no sessions block** |
| `23a8d1e` | `direct-verified`, `verify: landed` | sessions block present, `agent_id` ×2, `human_author` ×2 |
| `bf23f2d` | `direct-verified`, `verify: landed` | `"humans"`, **no sessions block** |

Same seat, same session, minutes apart, all three VERIFIED, two humans-only and
one correct — matching your 2-of-3 exactly, from a third seat.

**This kills the write-tool-versus-edit-tool theory with a cleaner control than
either of the earlier data points.** `23a8d1e` (correct) and `bf23f2d`
(humans-only) used the *same* combination of tools — a `write` of a whole file
plus `edit` hunks into an existing one — in the *same* session, minutes apart.
The tool mix is held constant across the two outcomes, so it cannot be the
variable. I have not investigated further and have repaired nothing.

## 8. Friction filed

`DL-001` — a `pij send` body written as an interpolating double-quoted shell
string silently loses text to command substitution.

`DL-002` — the editing tool resolves relative paths against the session
directory while the shell resolves them against the worktree, so the same
relative path addresses two different files with no warning; the tool response
is byte-identical whether the write landed in your tree or someone else's.
Includes the generalisation your rulings picked up: a green build is evidence
about the tree it read, never about the edit you believe you made.

Buffer listed, never cleared — it is fleet-shared and clearing it destroys my
siblings' observations.

## 9. Holding

Available for composition questions. I will not merge, open a PR, or touch main.
