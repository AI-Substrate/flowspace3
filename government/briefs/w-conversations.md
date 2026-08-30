# Brief: w-conversations — execute the conversations v1 plan (Jordan ruled 2026-08-27)

**Seat**: (fill at canary — fresh seat; conversations become your domain). This packet
executes an EXISTING PLAN — you are the implementer, not the designer.

## Your sources of truth, in authority order

1. `docs/plans/conversations/` — the dd-native plan: 3 phases, 17 tasks, 10 ACs.
   Read plan.dd.md + all three assets/tasks/phase-N/tasks.dd.md FULLY. Task text
   carries file:line evidence from a research pass and an adversarial critic pass —
   the traps named there (ElementKind CHECK + Rust enum, the FIVE reference-predicate
   sites, the element-resolution LATERAL, unanchored-conversation identity) are real
   and already cost analysis to find; do not rediscover them the hard way.
2. `docs/plans/prd/workshops/005-conversations.md` — AUTHORITATIVE design (tables,
   measured payload, scope). 003 (query surface) + 004 (envelopes) bind where cited.
3. AGENTS.md (dogfood + observe duties) and how-we-work.md §10.

## Working shape

- Own worktree `../fs3-conversations` + branch `w-conversations` off fresh main.
- **One PR per phase, in order** (store → daemon → CLI); each phase's done_when is its
  PR bar: harness checks green (7 gates; first run against a brand-new
  FS3_TEST_DATABASE_URL may false-negative the test gate — re-run once; canonical
  `postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test`), conventional
  commits (`feat:`), report the PR number, never self-merge. Update task states in the
  plan via `ddocs` as you land them (commit the .dd.json + generated .md together).
- Production-database ruling binds: tests never touch the default 5433 database; do
  not run a daemon against the production DB from your worktree.
- Migration numbers: the plan says take the NEXT FREE on freshly-pulled main at the
  moment you write it — 0012 is claimed by the in-flight w-update-truth packet; check
  what has MERGED when you get there and coordinate through o-prime on collision.
- Coordinate note: w-get-verb (pij-clumsy-tick) is building get/tree with a conv:
  dispatch arm answering FS3-E-QUERY-NOT-IMPLEMENTED — your phase 3 replaces that arm
  IF its PR has merged by then; if not, ship `conversation show` with the same
  contract and o-prime reconciles. Do not touch its fence
  (core address type, daemon get.rs/tree.rs) beyond filling the designed arm.

## Deviations

Ack with your read + numbered plan-of-attack per phase BEFORE coding. Real design
discoveries mid-work are stop-and-asks with evidence; corrections to the plan with
file:line proof outrank the plan (it was written from research, not from running your
code). Element splitting, live capture, rollups stay OUT regardless of temptation.
