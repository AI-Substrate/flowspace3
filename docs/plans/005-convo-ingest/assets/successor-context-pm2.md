# Successor context — PM seat 2 (predecessor pij-linguistic-narwhal died 2026-08-28T00:30Z)

Your predecessor was killed by a machine-level event mid-phase-1 (unrequested,
pid gone; NOT a work failure — its last acts were high quality). You inherit
its seat: same packet (packet-pm.dd.md), same worktree, same rulings. Its full
session transcript is readable at
~/.omp/agent/sessions/-substrate-flowspace-fs3-convo-ingest/2026-08-28T00-09-00-542Z_01a045b2-cb7e-7000-9148-24d70a01bb8c.jsonl
— skim its tail if you need its in-flight reasoning; the durable record below
is authoritative.

## Standing rulings (all recorded, all in effect)
- Prime rulings A-G at ack (vendored inputs; sanitizer spec + credential grep;
  throwaway harvesters OK; first-light on YOUR OWN session first, sealed
  scratch PG only; migration 0014 re-verified before write; the BUILT intake
  surface is binding over the telemetry sample; settings models confirmed).
- SA1: readers live in PROVIDERS (crates/providers/src/conversation_sources/),
  parsers stays pure, allowlist row = providers -> rusqlite.
- SA2: ConversationSource is the ruled third port; ports.rs guard now reads
  "A fourth port is stop-and-ask."
- PM ruling (kept): line framing lives ONCE in conversation_sources/tail.rs.
- Coders must export PIJ_SESSION_ID=<their id> to pij send from worktrees.
- Compose collision pre-emption goes in every coder packet (container_name is
  pinned; point coders at the shared db on :5433, no docker compose up).

## State at death (verify, don't trust — git status is your evidence)
- COMMITTED on branch: plan + tasks + packet-pm + impl-guide WITH amendments
  (SA1/SA2, tail.rs ruling, measured recipe corrections a-d) + assets/inputs/
  vendored (sha-pinned) + fixture-sanitizer-spec.md.
- UNCOMMITTED on disk (phase 1, near-complete): crates/core/src/
  conversation_source.rs (+ lib.rs, ports.rs edits), crates/providers/src/
  conversation_sources/ skeleton (+ lib.rs edit), testkit: conversation_source.rs,
  fake_source.rs, tests/conversation_source_contract.rs, fixtures/conversations/,
  arch-allowlist.toml row. Harvesters had finished or nearly finished.
- NOT yet done: phase-1 commit, oracle expectations (tk-c105) status unknown,
  FREEZE ANNOUNCEMENT to prime (required before any coder spawn), ddocs task
  checks for c101-c105.

## Your first moves
1. Canary to prime as instructed, then: read packet + impl-guide (committed
   amendments included) + this note; inventory the uncommitted tree against
   tk-c101..c105; run the contract suite + harness checks (FS3_TEST_DATABASE_URL
   per ruling D(ii) — scratch db fs3_convo_ingest exists); fix or finish what
   the evidence says is unfinished.
2. Commit phase 1 (conventional commits, harness commit), check tasks/ACs via
   ddocs with receipts, then send prime the FREEZE ANNOUNCEMENT.
3. Proceed to fan-out per packet i4 (4 seats) with the PIJ_SESSION_ID and
   compose pre-emptions in every coder packet.

One more thing: the pij alias-minting defect (pij#19) fired under your
predecessor's parallel harvesters (4 phantom ids). If you run parallel
subprocesses that invoke pij verbs, expect phantom ready-pings/tombstones —
prime ignores them; you should too.
