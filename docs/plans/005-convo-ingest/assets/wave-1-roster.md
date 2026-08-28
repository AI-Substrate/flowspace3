# Wave-1 roster and live state — plan 005-convo-ingest

Written by the PM seat so that a successor inherits FACTS rather than a
reconstruction. My predecessor died mid-phase-1 and the expensive part of
recovery was not the code — it was that nothing on disk distinguished
"written and passing" from "written and never run", and nothing recorded who
was doing what. This file is the fix. Update it at every seat edge.

## Seats

| unit | responsibility | seat | spawnId | worktree | branch | scratch db |
| --- | --- | --- | --- | --- | --- | --- |
| pm | phase 1, composition, review | **pij-pale-silkworm** (PM3) | s1787880073435-84015 | ../fs3-convo-ingest | 005-convo-ingest | fs3_convo_ingest |
| u1a | claude-native reader | **pij-frightened-mastodon** | s1787880412627-13542 | ../fs3-convo-u1a | 005-convo-u1a | fs3_convo_u1a |
| u1b | omp reader + pij-ledger reader | **pij-suitable-cormac** | s1787880413196-13981 | ../fs3-convo-u1b | 005-convo-u1b | fs3_convo_u1b |
| u1d | metrics-db reader | **pij-causal-mollusk** | s1787880413791-15131 | ../fs3-convo-u1d | 005-convo-u1d | fs3_convo_u1d |
| u2 | cursor-state service + normalizer | pij-appalling-slug | s1787878504607-39845 | ../fs3-convo-u2 | 005-convo-u2 | fs3_convo_u2 |

All four coders: omp, `github-copilot/claude-opus-5`, effort high, per
`.harness/government/settings.dd.md`. Prime is pij-instant-lynx; the PM is the
coders' only upward interface.

Seat-to-unit was assigned by spawn order and confirmed by asking each seat to
contradict me if its own `--task` named a different unit — there is an active
pij alias-minting defect (pij#19), so identity is verified, never assumed.

For the second dispatch that verification was WEAKER than it looks, and both
affected seats said so unprompted. `pij whoami` resolves a worktree-resident
seat to the MAIN CLONE folder, not its own worktree; and a seat's
`~/.pij/<id>/events.ndjson` carries no spawn-task record, so a coder cannot
read its own assignment back out of pij. Each seat's corroboration of its unit
was therefore circumstantial — the worktree it was sent to, that worktree's
branch, and the scope row of the packet it read, all three agreeing — rather
than a fact pij could confirm. Three seats, three independent agreements, no
contradictions; good enough to proceed, and recorded as friction (it is the
second time this gap has cost a seat something).

### Seat history — three PMs and a second wave-1 dispatch

| seat | unit | fate |
| --- | --- | --- |
| pij-linguistic-narwhal | pm (PM1) | killed by a machine event mid-phase-1, 00:30Z |
| pij-traditional-piranha | pm (PM2) | closed and committed phase 1, wrote the coder packets, dispatched wave 1; killed by the disk exhaustion at 01:07Z |
| pij-leading-worm | u1a | killed 01:07Z; **left nothing** |
| pij-delicious-salmon | u1b | killed 01:07Z; **left nothing** |
| pij-distinct-limpet | u1d | killed 01:07Z; **left nothing** |

The three dead coder worktrees were inventoried with git before re-dispatch —
zero commits past the fan-out commit `460883d`, clean trees, no file touched
since dispatch. The successors started clean from the same packets plus
`assets/wave-1-restart-note.md`; there were no deltas to reconcile. This
CORRECTS `successor-context-pm3.md`, which recorded u1b and u1d as leaving
partial work with sources intact; prime accepted the correction as the record.

u2 survived the sweep and finished. Its seat is alive and holding for
composition questions.

## Phase state

**Phase 1 — CLOSED 2026-08-28.** Commits `a3bbfd2` (the seam) and `f32b45c`
(ddoc close-out with receipts). `harness checks` green against the sealed
scratch db; `harness plan validate` 0 errors, 6 warnings that are all the same
honest mid-plan shape — checked phase-1 tasks pointing at ac-0007/ac-0003,
which phases 2 and 3 close. tk-c101..c105 all checked, each carrying its
evidence in its receipt.

**Phase 2 — dispatched.** Packets committed at `e6f5b44` as
`packet-coder-{u1a,u1b,u1d,u2}.dd.md`, delivered to the seats as PATHS. Every
seat holds at ack until the PM rules its numbered plan.

**Phase 3 — not started.** PM composes; unit-internal rework at composition
time is a phase-1 contract defect, not a composition task.

## What is frozen

`fs3_core::conversation_source` — the `ConversationSource` trait and its types
(`Harness`, `IngestInput`, `SessionKind`, `SessionFile`, `SourceCursor`,
`RawRecord`, `ReadBatch`); `fs3_testkit::conversation_source` — the five-case
contract suite; `fs3_testkit::expectations` and the four
`crates/testkit/fixtures/conversations/*/expectations.json`;
`fs3_providers::conversation_sources::tail` — line framing, written once.

A coder needing something the trait does not have raises it with the PM. A
change to the frozen shape goes to prime. After the freeze, a contract change
is a defect, not a refactor.

## Standing rulings in force

- Prime A–G at PM ack: inputs vendored under `assets/inputs/` and sha-pinned;
  sanitizer spec plus credential grep binding on every committed fixture;
  throwaway harvesters acceptable; first light runs against the PM's OWN
  session first, sealed scratch PG only; migration 0014 re-verified against
  freshly-pulled main before it is written; the BUILT intake surface outranks
  the telemetry sample, which is its rationale; settings model defaults
  confirmed.
- SA1 — readers are IO port impls and live in `crates/providers`, not
  `crates/parsers`, which stays pure. Allow-list row: providers → rusqlite.
- SA2 — `ConversationSource` is fs3's third port; the `ports.rs` guard now
  reads "A fourth port is stop-and-ask." Blocking, not async: every impl is
  file or sqlite IO, so the composition root hands it to `spawn_blocking`.
- PM, phase 1 — incremental line framing lives ONCE in
  `conversation_sources/tail.rs`, not three times in three readers. u2's
  cursor-state owns what is genuinely durable: persistence between runs, and
  the ordinal ledger a post-rotation rescan is deduplicated against.
- PM + prime, tk-c105 — the pinned `reconvo.py` is a SUBSET oracle, not an
  equality one; it cannot read the claude store at all, so claude carries a
  structural claim labelled PM-derived-not-oracle; only prose kinds
  (assistant, human, pij_in) are comparable by text. Full statement in the
  impl-guide's architecture section, where coders will actually read it.
- Coders export `PIJ_SESSION_ID` before messaging from a worktree, or the
  reply is silently lost.
- Nobody runs `docker compose up`: `container_name` is pinned and Postgres is
  already up for the fleet on host port 5433.

## Known live defects worked around

- pij alias-minting (pij#19) can mint phantom ids under parallel subprocesses
  that invoke pij verbs — expect stray ready-pings and tombstones; prime
  ignores them and so do we. Seat identity is confirmed by asking the seat.
