# Retro 2026-09-02 — plans 010 (embed-cap-heal) and 011 (conv-verify): the drain

Drained by o-prime (pij-instant-lynx). Sources: limpet (8), zealot (7), fiscal-tick (1), top-sloth (0), o-prime (see scratch/closeout-010-011/buffer-oprime.txt). Raw buffers vendored at scratch/closeout-010-011/. Seats LISTED, never cleared; o-prime clears its own after this record lands.

## The run in one paragraph
Two single-unit plans, two GPT-5.6 coders, two Claude reviewers, no PM, on an unstable pij (every omp seat landed in rs and could not message its legacy prime — req-0034). Both shipped to prod the same day with cross-model review that found real defects (a missing adapter in the heal; an ask that resolved but retrieved nothing; a 500 for a correct negative; a guard test that never reached its guard), each fix mutation-checked individually, each plan closed by REAL USAGE: prod drain + prod envelopes + a peer government's consumer read-back. Cost: one prod postgres crash (our own test helper), one lost gate to a lying tripwire, ~7 minutes of a coder waiting on a watcher blind spot.

## Observations, grouped, with the encode-next per group

### A. The wire (pij split) — 3 obs (limpet DL-001; o-prime truncation; the fourfold relay)
- rs child cannot `pij send` its legacy parent; E-RS text blames the parent and says "adopt". → upstream req-0034 (minted). OURS: file+poll channel is now the packet contract (`.harness/temp/agent/<packet>-*.md`), `pij-rs send --msg-id` for prime→child, generation recorded at canary (how-we-work 2b).
- pij-rs send truncates long inline bodies (reviewer lost the tail at "#4 is MY au…"). → pointer delivery for anything over a few lines; observe filed.
- ENCODE: a `harness team channel <seat>` helper that writes/polls the file channel so no seat improvises it.

### B. Tools that report success and lie — 5 obs (zealot ×3 LSP, limpet gate opacity, o-prime bounce/health)
- Rust LSP `references` missed a direct caller; LSP `rename` reported an edit it did not make, then inserted a stray token causing a syntax error while reporting success. → coders: grep after every LSP mutation; packet i8 gains "verify every LSP edit by exact read".
- `harness checks` is an opaque multi-minute job with no stage output — slow vs stuck undistinguishable (load 124). → ENCODE: checks must stream its stage; row candidate.
- `harness daemon bounce` returned ok while the daemon listened but did not serve for ~2 min; the CLI's DAEMON-UNAVAILABLE fix text tells a human to start a second daemon. → ENCODE: bounce waits for /health (bounded) and reports "booting"; the fix text checks for a listening pid first. Row 130.
- The prod-tripwire said "absent" for "unreachable" during a postgres recovery. → row 124.

### C. Test infrastructure vs the shared postgres — 4 obs (limpet ×2, zealot ×1, fiscal-tick ×1)
- `harness boot` reports compose db down; `docker compose up -d db` collides on the fixed container name (FOURTH seat to hit it). → row 110 family.
- `FreshDatabase` fires ~12 concurrent CREATE DATABASE per test binary and crashed the shared postmaster (four occurrences on record). → row 126: serialise in testkit; `--test-threads=2` until then. Separate test postmaster longer term (row 124b).
- The prod tripwire cannot tell dropped from unreachable. → row 124.

### D. Plan/skill text that was wrong — 4 obs (limpet ×2, zealot ×1, o-prime)
- builder implement module mandates `node_modules/.bin/dd`; this repo uses global `ddocs` (two seats hit it). → fix the skill text (Jordan's global skill); packet i3 names `ddocs` explicitly meanwhile.
- o-prime's impl-guides did not validate (17 errors each: strings where the schema wants objects). → ENCODE: `harness team new` runs `harness plan validate` on the scaffold, and o-prime validates before dispatch (added to the dispatch ritual).
- ac-0006's premise was false (`conv:recovery` read as a conversation job). → amended honestly; ENCODE: recovery-job provenance in the payload/key so `conv:recovery` cannot be misread (row 128 sibling).
- Reviewer's "two try_post" trap was wrong and o-prime relayed it approvingly. → verify-then-relay applies to reviewer text; reviewer packet i4 gains the receipt discipline.

### E. Search honesty — 3 obs (zealot latency, limpet scoped-zero, o-prime row 119)
- Search 23–120s under host load; DB-bound on postgres defaults (128MB buffers vs 2.3GB HNSW). → row 122.
- A cwd-scoped zero read as absence (limpet's conversation search; o-prime's conv: get). → rows 119 (shipped in #93 for conv:), 129; ENCODE: every scoped-zero envelope names the scope and the widening flag.

## What we shipped from this run's own findings
- #93: conv: addresses authoritative; two-message miss; `conversation verify` — closed row 119 and gave meadowlark's backfill its probe.
- #92: cap-rejection heal across four adapters; FILL alignment — closed row 117; four prod jobs drained.
- Rulings: small reversible prod repairs are o-prime's (2026-09-02); sol codes / Claude reviews; generation-at-canary.

## Encode-next, ranked by seats-per-day it saves
1. Row 126 — serialise FreshDatabase (every gate on the box).
2. Bounce waits for health + fix-text guard (row 130) — a human hit it today.
3. `harness checks` stage streaming (row 131).
4. Scaffold-time plan validation in `harness team new`.
5. Skill text: `ddocs` not `node_modules/.bin/dd`; reviewer packet receipt discipline; coder packet LSP-verify line.
